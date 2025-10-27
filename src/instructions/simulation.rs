// Simulation interface - for Aril platform frontend preview
use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::LendingError;
use crate::utils::{calculate_health_factor, get_sol_price};
use crate::instructions::valuation::{parse_personal_position_data, parse_clmm_current_tick, calculate_token_amounts_from_liquidity, calculate_value_from_token_amounts};

// ============ Simulation Data Structures ============

/// Simulation operation result
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SimulationResult {
    pub success: bool,
    pub new_health_factor: u64,
    pub new_collateral_value: u64,
    pub new_debt_value: u64,
    pub new_available_borrow: u64,
    pub error_message: String,
}

/// Simulation deposit result
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SimulateDepositResult {
    pub success: bool,
    pub new_user_deposit: u64,
    pub new_health_factor: u64,
    pub new_earned_interest: u64,
    pub error_message: String,
}

/// Simulation withdraw result
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct SimulateWithdrawResult {
    pub success: bool,
    pub new_user_deposit: u64,
    pub new_health_factor: u64,
    pub withdrawable_amount: u64,
    pub error_message: String,
}

// ============ Simulation Borrow Functionality ============

/// Simulate borrow operation
#[derive(Accounts)]
pub struct SimulateBorrow<'info> {
    /// User position account
    #[account(
        constraint = user_position.owner == user.key() @ LendingError::InvalidAuthority
    )]
    pub user_position: Account<'info, UserPosition>,

    /// Protocol configuration account
    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// Traditional USDC pool account
    #[account(
        seeds = [b"traditional_usdc_pool", protocol_config.usdc_mint.as_ref()],
        bump
    )]
    pub traditional_usdc_pool: Account<'info, TraditionalUsdcPool>,

    /// Pool USDC vault account (check liquidity)
    #[account(
        seeds = [b"traditional_usdc_vault", traditional_usdc_pool.key().as_ref()],
        bump
    )]
    pub pool_usdc_vault: Account<'info, anchor_spl::token::TokenAccount>,

    /// Chainlink SOL price feed
    /// CHECK: Chainlink price feed
    pub chainlink_sol_feed: UncheckedAccount<'info>,

    /// Chainlink program account
    /// CHECK: Chainlink program
    pub chainlink_program: UncheckedAccount<'info>,

    /// Raydium CLMM pool account
    /// CHECK: Raydium CLMM pool
    pub raydium_pool: UncheckedAccount<'info>,

    /// User NFT position account
    /// CHECK: User NFT position
    pub user_nft_position: UncheckedAccount<'info>,

    /// User account
    pub user: Signer<'info>,
}

/// Simulate borrow operation
pub fn simulate_borrow(ctx: Context<SimulateBorrow>, amount: u64) -> Result<SimulationResult> {
    // check basic conditions
    if amount == 0 {
        return Ok(SimulationResult {
            success: false,
            new_health_factor: ctx.accounts.user_position.health_factor_cache,
            new_collateral_value: ctx.accounts.user_position.collateral_value_cache,
            new_debt_value: ctx.accounts.user_position.usdc_borrowed,
            new_available_borrow: 0,
            error_message: "借款金额不能为0".to_string(),
        });
    }

    // check protocol liquidity
    if ctx.accounts.pool_usdc_vault.amount < amount {
        return Ok(SimulationResult {
            success: false,
            new_health_factor: ctx.accounts.user_position.health_factor_cache,
            new_collateral_value: ctx.accounts.user_position.collateral_value_cache,
            new_debt_value: ctx.accounts.user_position.usdc_borrowed,
            new_available_borrow: 0,
            error_message: "协议流动性不足".to_string(),
        });
    }

    // calculate current raw collateral value
    let raw_collateral_value = match calculate_current_collateral_value(&ctx) {
        Ok(value) => value,
        Err(_) => {
            return Ok(SimulationResult {
                success: false,
                new_health_factor: 0,
                new_collateral_value: 0,
                new_debt_value: ctx.accounts.user_position.usdc_borrowed,
                new_available_borrow: 0,
                error_message: "无法计算抵押品价值".to_string(),
            });
        }
    };

    // apply 80% discount factor to get effective collateral value (consistent with update_collateral_value)
    use crate::utils::apply_discount;
    use crate::state::DISCOUNT_FACTOR;
    let effective_collateral_value = match apply_discount(raw_collateral_value, DISCOUNT_FACTOR) {
        Ok(value) => value,
        Err(_) => {
            return Ok(SimulationResult {
                success: false,
                new_health_factor: ctx.accounts.user_position.health_factor_cache,
                new_collateral_value: raw_collateral_value,
                new_debt_value: ctx.accounts.user_position.usdc_borrowed,
                new_available_borrow: 0,
                error_message: "折扣计算失败".to_string(),
            });
        }
    };

    // calculate new total debt
    let new_total_debt = match ctx.accounts.user_position.usdc_borrowed.checked_add(amount) {
        Some(debt) => debt,
        None => {
            return Ok(SimulationResult {
                success: false,
                new_health_factor: ctx.accounts.user_position.health_factor_cache,
                new_collateral_value: effective_collateral_value,
                new_debt_value: ctx.accounts.user_position.usdc_borrowed,
                new_available_borrow: 0,
                error_message: "债务金额溢出".to_string(),
            });
        }
    };

    // calculate max allowed total borrow amount (based on effective collateral value, using 50% borrow rate)
    let collateral_value_6_decimals = effective_collateral_value / 100; // convert to 6 decimal places
    let max_total_borrow_amount = collateral_value_6_decimals *
                                 ctx.accounts.protocol_config.collateral_ratio as u64 / 10000;

    // check if exceeds borrow limit
    if new_total_debt > max_total_borrow_amount {
        return Ok(SimulationResult {
            success: false,
            new_health_factor: ctx.accounts.user_position.health_factor_cache,
            new_collateral_value: effective_collateral_value,
            new_debt_value: ctx.accounts.user_position.usdc_borrowed,
            new_available_borrow: if max_total_borrow_amount > ctx.accounts.user_position.usdc_borrowed {
                max_total_borrow_amount - ctx.accounts.user_position.usdc_borrowed
            } else {
                0
            },
            error_message: "Exceeds borrow limit".to_string(),
        });
    }

    // calculate new health factor
    let new_health_factor = calculate_health_factor(collateral_value_6_decimals, new_total_debt);

    // calculate new available borrow amount
    let new_available_borrow = if max_total_borrow_amount > new_total_debt {
        max_total_borrow_amount - new_total_debt
    } else {
        0
    };

    Ok(SimulationResult {
        success: true,
        new_health_factor,
        new_collateral_value: effective_collateral_value,
        new_debt_value: new_total_debt,
        new_available_borrow,
        error_message: "".to_string(),
    })
}

// ============ Simulate Repay Function ============

/// Simulate repay operation
#[derive(Accounts)]
pub struct SimulateRepay<'info> {
    /// User position account
    #[account(
        constraint = user_position.owner == user.key() @ LendingError::InvalidAuthority
    )]
    pub user_position: Account<'info, UserPosition>,

    /// Protocol configuration account
    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// User account
    pub user: Signer<'info>,
}

/// Simulate repay operation
pub fn simulate_repay(ctx: Context<SimulateRepay>, amount: u64) -> Result<SimulationResult> {
    let user_position = &ctx.accounts.user_position;

    if amount == 0 {
        return Ok(SimulationResult {
            success: false,
            new_health_factor: user_position.health_factor_cache,
            new_collateral_value: user_position.collateral_value_cache,
            new_debt_value: user_position.usdc_borrowed,
            new_available_borrow: 0,
            error_message: "Repay amount cannot be zero".to_string(),
        });
    }

    if amount > user_position.usdc_borrowed {
        return Ok(SimulationResult {
            success: false,
            new_health_factor: user_position.health_factor_cache,
            new_collateral_value: user_position.collateral_value_cache,
            new_debt_value: user_position.usdc_borrowed,
            new_available_borrow: 0,
            error_message: "Repay amount exceeds current debt".to_string(),
        });
    }

    // Calculate new debt value
    let new_debt = user_position.usdc_borrowed - amount;

    // Calculate new health factor - using effective collateral value (80% discount applied)
    let effective_collateral_6_decimals = user_position.collateral_value_usd / 100;
    let new_health_factor = if new_debt > 0 {
        calculate_health_factor(effective_collateral_6_decimals, new_debt)
    } else {
        u64::MAX // Health factor is max when no debt
    };

    // Calculate new available borrow amount - based on effective collateral value
    let max_total_borrow_amount = effective_collateral_6_decimals *
                                 ctx.accounts.protocol_config.collateral_ratio as u64 / 10000;
    let new_available_borrow = if max_total_borrow_amount > new_debt {
        max_total_borrow_amount - new_debt
    } else {
        0
    };

    Ok(SimulationResult {
        success: true,
        new_health_factor,
        new_collateral_value: user_position.collateral_value_usd, // Return effective collateral value
        new_debt_value: new_debt,
        new_available_borrow,
        error_message: "".to_string(),
    })
}

// ============ Simulate Deposit Function ============

/// Simulate deposit operation
#[derive(Accounts)]
pub struct SimulateDeposit<'info> {
    /// Traditional USDC pool account
    #[account(
        seeds = [b"traditional_usdc_pool", usdc_mint.key().as_ref()],
        bump
    )]
    pub traditional_usdc_pool: Account<'info, TraditionalUsdcPool>,

    /// USDC mint account
    /// CHECK: USDC mint address
    pub usdc_mint: UncheckedAccount<'info>,

    /// User account
    pub user: Signer<'info>,
}

/// Simulate deposit operation
pub fn simulate_deposit(ctx: Context<SimulateDeposit>, amount: u64) -> Result<SimulateDepositResult> {
    if amount == 0 {
        return Ok(SimulateDepositResult {
            success: false,
            new_user_deposit: 0,
            new_health_factor: u64::MAX,
            new_earned_interest: 0,
            error_message: "Deposit amount cannot be zero".to_string(),
        });
    }

    if amount < ctx.accounts.traditional_usdc_pool.min_deposit_amount {
        return Ok(SimulateDepositResult {
            success: false,
            new_user_deposit: 0,
            new_health_factor: u64::MAX,
            new_earned_interest: 0,
            error_message: "Deposit amount below minimum required".to_string(),
        });
    }

    // Simulate deposit success
    Ok(SimulateDepositResult {
        success: true,
        new_user_deposit: amount,
        new_health_factor: u64::MAX, // Deposit does not affect health factor (no debt)
        new_earned_interest: 0, // New deposit has no interest
        error_message: "".to_string(),
    })
}

// ============ Simulate Withdraw Function ============

/// Simulate withdraw operation
#[derive(Accounts)]
pub struct SimulateWithdraw<'info> {
    /// User deposit record
    #[account(
        constraint = deposit_record.owner == user.key() @ LendingError::InvalidAuthority
    )]
    pub deposit_record: Account<'info, UserDepositRecord>,

    /// Traditional USDC pool account
    #[account(
        constraint = traditional_usdc_pool.key() == deposit_record.pool @ LendingError::InvalidAuthority
    )]
    pub traditional_usdc_pool: Account<'info, TraditionalUsdcPool>,

    /// User account
    pub user: Signer<'info>,
}

/// Simulate withdraw operation
pub fn simulate_withdraw(ctx: Context<SimulateWithdraw>, amount: u64) -> Result<SimulateWithdrawResult> {
    let deposit_record = &ctx.accounts.deposit_record;

    if amount == 0 {
        return Ok(SimulateWithdrawResult {
            success: false,
            new_user_deposit: deposit_record.principal_amount,
            new_health_factor: u64::MAX,
            withdrawable_amount: 0,
            error_message: "Withdraw amount cannot be zero".to_string(),
        });
    }

    if deposit_record.is_withdrawn {
        return Ok(SimulateWithdrawResult {
            success: false,
            new_user_deposit: 0,
            new_health_factor: u64::MAX,
            withdrawable_amount: 0,
            error_message: "Deposit has been withdrawn".to_string(),
        });
    }

    // Calculate current withdrawable amount (principal + interest)
    // For deposit record, need to calculate interest based on deposit time and current rate
    let current_interest = calculate_deposit_interest(
        deposit_record.principal_amount,
        ctx.accounts.traditional_usdc_pool.current_deposit_rate,
        deposit_record.deposit_timestamp,
    ).unwrap_or(0);

    let total_withdrawable = deposit_record.principal_amount + current_interest;

    if amount > total_withdrawable {
        return Ok(SimulateWithdrawResult {
            success: false,
            new_user_deposit: deposit_record.principal_amount,
            new_health_factor: u64::MAX,
            withdrawable_amount: total_withdrawable,
            error_message: "Withdraw amount exceeds available balance".to_string(),
        });
    }

    // Calculate remaining amount after withdraw
    let remaining_amount = if amount >= total_withdrawable {
        0 // Withdraw all
    } else {
        total_withdrawable - amount
    };

    Ok(SimulateWithdrawResult {
        success: true,
        new_user_deposit: remaining_amount,
        new_health_factor: u64::MAX, // Traditional deposit does not affect health factor
        withdrawable_amount: total_withdrawable,
        error_message: "".to_string(),
    })
}

// ============ Simulate Supply Collateral Function ============

/// Simulate LP token deposit operation (supply collateral)
/// Simulate increasing liquidity in existing CLMM position for SOL and USDC
#[derive(Accounts)]
pub struct SimulateSupplyCollateral<'info> {
    /// User position account
    #[account(
        constraint = user_position.owner == user.key() @ LendingError::InvalidAuthority
    )]
    pub user_position: Account<'info, UserPosition>,

    /// Protocol configuration
    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// Chainlink SOL价格feed
    /// CHECK: Chainlink price feed
    pub chainlink_sol_feed: UncheckedAccount<'info>,

    /// Chainlink program ID
    /// CHECK: Chainlink program
    pub chainlink_program: UncheckedAccount<'info>,

    /// Raydium CLMM pool account
    /// CHECK: Raydium CLMM pool
    pub raydium_pool: UncheckedAccount<'info>,

    /// User NFT position account
    /// CHECK: User NFT position
    pub user_nft_position: UncheckedAccount<'info>,

    /// User account
    pub user: Signer<'info>,
}

/// Simulate LP token deposit operation (supply collateral)
pub fn simulate_supply_collateral(
    ctx: Context<SimulateSupplyCollateral>,
    sol_amount: u64,  // lamports
    usdc_amount: u64, // microUSDC 
) -> Result<SimulationResult> {
    if sol_amount == 0 && usdc_amount == 0 {
        return Ok(SimulationResult {
            success: false,
            new_health_factor: ctx.accounts.user_position.health_factor_cache,
            new_collateral_value: ctx.accounts.user_position.collateral_value_cache,
            new_debt_value: ctx.accounts.user_position.usdc_borrowed,
            new_available_borrow: 0,
            error_message: "SOL和USDC数量不能都为0".to_string(),
        });
    }

    // 1. Get current liquidity data for user position
    let current_collateral_value = match calculate_current_collateral_value_for_supply(&ctx) {
        Ok(value) => value,
        Err(_) => {
            return Ok(SimulationResult {
                success: false,
                new_health_factor: 0,
                new_collateral_value: 0,
                new_debt_value: ctx.accounts.user_position.usdc_borrowed,
                new_available_borrow: 0,
                error_message: "Cannot get current position value".to_string(),
            });
        }
    };

    // 2. Get SOL price from Chainlink feed
    let sol_price = match get_sol_price(&ctx.accounts.chainlink_program, &ctx.accounts.chainlink_sol_feed) {
        Ok(price) => price,
        Err(_) => {
            return Ok(SimulationResult {
                success: false,
                new_health_factor: ctx.accounts.user_position.health_factor_cache,
                new_collateral_value: ctx.accounts.user_position.collateral_value_cache,
                new_debt_value: ctx.accounts.user_position.usdc_borrowed,
                new_available_borrow: 0,
                error_message: "无法获取SOL价格".to_string(),
            });
        }
    };

    // 3. Calculate additional liquidity value
    let usdc_price = 1_000_000u64; // $1.000000 
    let additional_value = match calculate_value_from_token_amounts(
        sol_amount,
        usdc_amount,
        sol_price,
        usdc_price,
    ) {
        Ok(value) => value,
        Err(_) => {
            return Ok(SimulationResult {
                success: false,
                new_health_factor: ctx.accounts.user_position.health_factor_cache,
                new_collateral_value: ctx.accounts.user_position.collateral_value_cache,
                new_debt_value: ctx.accounts.user_position.usdc_borrowed,
                new_available_borrow: 0,
                error_message: "Cannot calculate additional liquidity value".to_string(),
            });
        }
    };

    // 4. Calculate new total collateral value
    let new_collateral_value = current_collateral_value + additional_value;

    // 5. Calculate new health factor
    let collateral_value_6_decimals = new_collateral_value / 100;
    let new_health_factor = if ctx.accounts.user_position.usdc_borrowed > 0 {
        calculate_health_factor(collateral_value_6_decimals, ctx.accounts.user_position.usdc_borrowed)
    } else {
        u64::MAX
    };

    // 6. Calculate new available borrow amount
    let max_total_borrow_amount = collateral_value_6_decimals *
                                 ctx.accounts.protocol_config.collateral_ratio as u64 / 10000;
    let new_available_borrow = if max_total_borrow_amount > ctx.accounts.user_position.usdc_borrowed {
        max_total_borrow_amount - ctx.accounts.user_position.usdc_borrowed
    } else {
        0
    };

    Ok(SimulationResult {
        success: true,
        new_health_factor,
        new_collateral_value,
        new_debt_value: ctx.accounts.user_position.usdc_borrowed,
        new_available_borrow,
        error_message: "".to_string(),
    })
}

// ============ Supply Collateral Simulation Helpers ============

/// Calculate current collateral value (reuse existing logic)
fn calculate_current_collateral_value(ctx: &Context<SimulateBorrow>) -> Result<u64> {
    // Get SOL price from Chainlink feed
    let sol_price = get_sol_price(
        &ctx.accounts.chainlink_program,
        &ctx.accounts.chainlink_sol_feed,
    )?;

    // Parse PersonalPosition data
    let position_data = parse_personal_position_data(
        &ctx.accounts.user_nft_position.try_borrow_data()?
    )?;

    // Get current tick from Raydium pool
    let current_tick = parse_clmm_current_tick(
        &ctx.accounts.raydium_pool.try_borrow_data()?
    )?;

    // Calculate token amounts from liquidity
    let (token0_amount, token1_amount) = calculate_token_amounts_from_liquidity(
        position_data.liquidity,
        current_tick,
        position_data.tick_lower_index,
        position_data.tick_upper_index,
    )?;

    // Calculate total value
    calculate_value_from_token_amounts(
        token0_amount,
        token1_amount,
        sol_price,
        1_000_000 
    )
}

/// Calculate current collateral value for supply (reuse existing logic)
fn calculate_current_collateral_value_for_supply(ctx: &Context<SimulateSupplyCollateral>) -> Result<u64> {
    // Get SOL price from Chainlink feed
    let sol_price = get_sol_price(
        &ctx.accounts.chainlink_program,
        &ctx.accounts.chainlink_sol_feed,
    )?;

    // Parse PersonalPosition data
    let position_data = parse_personal_position_data(
        &ctx.accounts.user_nft_position.try_borrow_data()?
    )?;

    // Get current tick from Raydium pool
    let current_tick = parse_clmm_current_tick(
        &ctx.accounts.raydium_pool.try_borrow_data()?
    )?;

    // Calculate token amounts from liquidity
    let (token0_amount, token1_amount) = calculate_token_amounts_from_liquidity(
        position_data.liquidity,
        current_tick,
        position_data.tick_lower_index,
        position_data.tick_upper_index,
    )?;

    // Calculate total value
    calculate_value_from_token_amounts(
        token0_amount,
        token1_amount,
        sol_price,
        1_000_000 
    )
}
/// Calculate deposit interest
fn calculate_deposit_interest(
    principal: u64,
    deposit_rate: u16,
    deposit_timestamp: i64,
) -> Result<u64> {
    let current_time = Clock::get()?.unix_timestamp;
    let duration_seconds = current_time
        .checked_sub(deposit_timestamp)
        .ok_or(LendingError::InvalidPrice)? as u64;

    // Calculate interest: principal × deposit_rate × duration_seconds / (365 days × 86400 seconds/day × 10000)
    // 365 × 86400 = 31,536,000 seconds/year
    let interest = (principal as u128)
        .checked_mul(deposit_rate as u128)
        .ok_or(LendingError::ArithmeticOverflow)?
        .checked_mul(duration_seconds as u128)
        .ok_or(LendingError::ArithmeticOverflow)?
        .checked_div(31_536_000 * 10000)
        .ok_or(LendingError::ArithmeticOverflow)? as u64;

    Ok(interest)
}
