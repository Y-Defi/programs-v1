use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::LendingError;
use crate::utils::{get_sol_price, apply_discount, calculate_health_factor};
use crate::instructions::raydium_integration::{get_raydium_pool_data, RaydiumIntegration};
use std::str::FromStr;

/// Update collateral value instruction
/// Update user position's collateral value and health factor
#[derive(Accounts)]
pub struct UpdateCollateralValue<'info> {
    /// Can be any account, no specific permission required (public callable)
    pub caller: Signer<'info>,

    /// Protocol configuration account
    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused @ LendingError::ProtocolPaused
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// Target user position account
    #[account(
        mut,
        constraint = user_position.lp_deposited > 0 @ LendingError::UserPositionNotFound
    )]
    pub user_position: Account<'info, UserPosition>,

    /// Chainlink SOL/USD price feed account
    /// CHECK: Verify this is the correct Chainlink SOL/USD price feed account
    pub chainlink_sol_feed: AccountInfo<'info>,

    /// Chainlink program account
    /// CHECK: Verify this is the correct Chainlink program account
    pub chainlink_program: AccountInfo<'info>,

    /// CLMM pool state account
    /// CHECK: Used to parse current tick and pool information
    pub clmm_pool_state: AccountInfo<'info>,

    /// PersonalPosition account (derived from NFT mint)
    /// CHECK: Verify this is the correct PersonalPosition account for the user's LP mint
    pub personal_position: AccountInfo<'info>,
}

/// Update collateral value handler function
/// Recompute user's collateral value and health factor using full logic from liquidation_validation.rs
pub fn update_collateral_value(ctx: Context<UpdateCollateralValue>) -> Result<()> {

    let user_position = &mut ctx.accounts.user_position;


    // If user has no LP deposited, no need to update
    if user_position.lp_deposited == 0 {
        return Ok(());
    }

    // 1. Get current SOL price from Chainlink feed
    let sol_price = get_sol_price(&ctx.accounts.chainlink_program, &ctx.accounts.chainlink_sol_feed)?;

    // 2. Verify PersonalPosition address matches user's LP mint
    let nft_mint = user_position.lp_mint;
    require!(nft_mint != Pubkey::default(), LendingError::InvalidLpMint);

    let raydium_clmm_program = anchor_lang::solana_program::pubkey!("DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH");
    let (expected_position_addr, _position_bump) = Pubkey::find_program_address(
        &[
            b"position",
            nft_mint.as_ref(),
        ],
        &raydium_clmm_program
    );

    // Verify PersonalPosition address 
    require_eq!(
        ctx.accounts.personal_position.key(),
        expected_position_addr,
        LendingError::InvalidPersonalPosition
    );


    // 3. Deserialize PersonalPosition account data
    let personal_position_data = ctx.accounts.personal_position.try_borrow_data()?;
    if personal_position_data.len() == 0 {
        return Err(LendingError::InvalidAccount.into());
    }

    let position_info = parse_personal_position_data(&personal_position_data)?;


    // 4. Parse CLMM Pool current tick
    let clmm_pool_data = ctx.accounts.clmm_pool_state.try_borrow_data()?;
    let current_tick = parse_clmm_current_tick(&clmm_pool_data)?;


    // 5. Calculate token amounts from liquidity
    let (sol_amount, usdc_amount) = calculate_token_amounts_from_liquidity(
        position_info.liquidity,
        current_tick,
        position_info.tick_lower,
        position_info.tick_upper,
    )?;


    // 6. Calculate total value
    let usdc_price = 1_000_000u64; // $1.000000
    let clmm_position_value = calculate_value_from_token_amounts(
        sol_amount,
        usdc_amount,
        sol_price,
        usdc_price,
    )?;


    // 7. Verify value reasonableness
    require!(clmm_position_value > 0, LendingError::ZeroAmount);

    // 8. Apply discount factor to get effective collateral value
    let effective_collateral = apply_discount(clmm_position_value, DISCOUNT_FACTOR)?;

    // 9. Update user position state
    let old_collateral_cache = user_position.collateral_value_cache;
    let old_collateral_usd = user_position.collateral_value_usd;

    user_position.collateral_value_cache = clmm_position_value; 
    user_position.collateral_value_usd = effective_collateral;  

    // 10. Recalculate health factor - using 6 decimal precision
    let effective_collateral_6_decimals = effective_collateral / 100; 
    user_position.health_factor = calculate_health_factor(
        effective_collateral_6_decimals,
        user_position.usdc_borrowed,
    );

    // 11. Recalculate max borrow amount - using 50% discount factor
    use crate::utils::calculate_max_borrow_amount;
    let max_total_borrow = (effective_collateral_6_decimals as u128)
        .checked_mul(ctx.accounts.protocol_config.collateral_ratio as u128)
        .and_then(|v| v.checked_div(10000u128)) 
        .unwrap_or(0) as u64;

    user_position.max_borrow_amount = calculate_max_borrow_amount(
        effective_collateral_6_decimals, 
        ctx.accounts.protocol_config.collateral_ratio, // 50% discount factor
        user_position.usdc_borrowed // existing debt
    )?;

    // 12. Update cache timestamp
    user_position.last_cache_update = Clock::get()?.unix_timestamp;
    user_position.last_update_slot = Clock::get()?.slot;

    // 13. Emit collateral value update event
    emit!(CollateralValueUpdated {
        user: user_position.owner,
        old_value: old_collateral_usd,
        new_value: effective_collateral,
        health_factor: user_position.health_factor,
        sol_price: sol_price,
        updated_at: Clock::get()?.unix_timestamp,
    });


    Ok(())
}

/// Check user health status
#[derive(Accounts)]
pub struct CheckHealth<'info> {
    pub caller: Signer<'info>,
    
    #[account(
        constraint = user_position.lp_deposited > 0 @ LendingError::UserPositionNotFound
    )]
    pub user_position: Account<'info, UserPosition>,
    
    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
}

/// Check user health status
/// Returns: Health status code (0=healthy, 1=warning, 2=danger)
/// Test account struct for fetching SOL price from Chainlink oracle
#[derive(Accounts)]
pub struct TestGetSolPrice<'info> {
    pub caller: Signer<'info>,
    
    /// CHECK: We're reading data from this chainlink feed account
    pub chainlink_feed: AccountInfo<'info>,
    
    /// CHECK: This is the Chainlink program library
    pub chainlink_program: AccountInfo<'info>,
}

/// Test function to fetch SOL price from Chainlink oracle
pub fn test_get_sol_price(ctx: Context<TestGetSolPrice>) -> Result<u64> {
    
    // Call our get_sol_price utility function
    let sol_price_usd = get_sol_price(&ctx.accounts.chainlink_program, &ctx.accounts.chainlink_feed)?;
    
    
    Ok(sol_price_usd)
}

/// Check user health status
/// Returns: Health status code (0=healthy, 1=warning, 2=danger)
pub fn check_health(ctx: Context<CheckHealth>) -> Result<u8> {
    let user_position = &ctx.accounts.user_position;
    let protocol_config = &ctx.accounts.protocol_config;
    
    if user_position.usdc_borrowed == 0 {
        return Ok(0); // No borrow, healthy
    }
    
    let status = determine_health_status(
        user_position.health_factor, 
        protocol_config.liquidation_threshold
    );
    
    emit!(HealthCheck {
        user: user_position.owner,
        health_factor: user_position.health_factor,
        status,
        checked_at: Clock::get()?.unix_timestamp,
    });
    
    Ok(status)
}


/// Check user health status
/// Returns: Health status code (0=healthy, 1=warning, 2=danger)
/// Test account struct for fetching SOL price from Chainlink oracle
#[derive(Accounts)]
pub struct CalculateClmmPositionValue<'info> {
    pub caller: Signer<'info>,
    
    /// Chainlink price feed account
    /// CHECK: verify account is a valid Chainlink price feed
    pub chainlink_feed: AccountInfo<'info>,
    
    /// Chainlink program account
    /// CHECK: verify account is a valid Chainlink program
    pub chainlink_program: AccountInfo<'info>,
    
    /// CLMM pool account
    /// CHECK: verify account is a valid CLMM pool account
    pub clmm_pool: AccountInfo<'info>,
    
    /// User's PersonalPosition account (derived from NFT mint)
    /// CHECK: verify account is a valid PersonalPosition account
    pub user_position: AccountInfo<'info>,
}

/// Calculate CLMM position value - main external interface
/// This is the main function external scripts should call
pub fn calculate_clmm_position_value(
    ctx: Context<CalculateClmmPositionValue>, 
    nft_mint: Pubkey  // Only need NFT mint address, other accounts are validated internally
) -> Result<u64> {
    
    // 1. Fetch current SOL price from Chainlink oracle
    let sol_price_usd = get_sol_price(&ctx.accounts.chainlink_program, &ctx.accounts.chainlink_feed)?;
    
    // 2. Derive PersonalPosition address from NFT mint and verify
    let raydium_clmm_program = Pubkey::from_str("DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH")
        .map_err(|_| LendingError::InvalidAccount)?;
    let (expected_position_addr, _bump) = Pubkey::find_program_address(
        &[
            b"position",
            nft_mint.as_ref(),
        ],
        &raydium_clmm_program
    );
    
    // Verify that the passed user_position account matches the derived address
    require_eq!(
        ctx.accounts.user_position.key(),
        expected_position_addr,
        LendingError::InvalidAccount
    );
    
    // 3. Parse PersonalPosition account data
    let position_info = parse_personal_position_data(&ctx.accounts.user_position.try_borrow_data()?)?;
    
    // 4. Parse CLMM pool data to get current tick
    let current_tick = parse_clmm_current_tick(&ctx.accounts.clmm_pool.try_borrow_data()?)?;
    
    // 5. Calculate token amounts from liquidity and price range
    let (token0_amount, token1_amount) = calculate_token_amounts_from_liquidity(
        position_info.liquidity,
        current_tick,
        position_info.tick_lower,
        position_info.tick_upper
    )?;
    
    
    // 6. Convert token amounts to USD value
    let total_value_usd = calculate_value_from_token_amounts(
        token0_amount,
        token1_amount,
        sol_price_usd,
        1_000_000
    )?;
    
    
    Ok(total_value_usd)
}

/// Deserialize PersonalPosition account data
pub fn parse_personal_position_data(account_data: &[u8]) -> Result<PersonalPositionInfo> {
    require!(account_data.len() >= 281, LendingError::InvalidAccount);
    
    // Skip account discriminator and bump byte (8 + 1 = 9)
    // These offsets are based on Raydium CLMM PersonalPositionState structure
    // discriminator (8) + bump (1) + nft_mint (32) + pool_id (32) = 73 bytes
    let mut tick_lower_bytes = [0u8; 4];
    tick_lower_bytes.copy_from_slice(&account_data[73..77]);
    let tick_lower = i32::from_le_bytes(tick_lower_bytes);
    
    let mut tick_upper_bytes = [0u8; 4];
    tick_upper_bytes.copy_from_slice(&account_data[77..81]);
    let tick_upper = i32::from_le_bytes(tick_upper_bytes);
    

    let mut liquidity_bytes = [0u8; 16];
    liquidity_bytes.copy_from_slice(&account_data[81..97]);
    let liquidity = u128::from_le_bytes(liquidity_bytes);
    
    Ok(PersonalPositionInfo {
        liquidity,
        tick_lower,
        tick_upper,
        // keep tick_lower_index and tick_upper_index for compatibility
        tick_lower_index: tick_lower,
        tick_upper_index: tick_upper,
    })
}

/// PersonalPosition account data structure
pub struct PersonalPositionInfo {
    pub liquidity: u128,
    pub tick_lower: i32,
    pub tick_upper: i32,
    // keep tick_lower_index and tick_upper_index for compatibility
    pub tick_lower_index: i32,
    pub tick_upper_index: i32,
}

/// Parse current tick from CLMM pool account data
pub fn parse_clmm_current_tick(pool_data: &[u8]) -> Result<i32> {
    require!(pool_data.len() >= 1544, LendingError::InvalidPoolData);
    
    // Based on Raydium CLMM pool structure, current_tick is at offset 269 (8 bytes)
    let mut tick_bytes = [0u8; 4];
    tick_bytes.copy_from_slice(&pool_data[269..273]);
    Ok(i32::from_le_bytes(tick_bytes))
}

/// Calculate token amounts from liquidity using Raydium CLMM official standard method
pub fn calculate_token_amounts_from_liquidity(
    liquidity: u128,
    current_tick: i32,
    tick_lower: i32,
    tick_upper: i32,
) -> Result<(u64, u64)> {
    if liquidity == 0 {
        return Ok((0, 0));
    }
    
    
    // Convert tick to sqrt price (Q64.64 format)
    let sqrt_ratio_lower = tick_to_sqrt_ratio_q64_64(tick_lower)?;
    let sqrt_ratio_upper = tick_to_sqrt_ratio_q64_64(tick_upper)?;
    let sqrt_ratio_current = tick_to_sqrt_ratio_q64_64(current_tick)?;
    
    
    // According to Raydium CLMM official standard, calculate token amounts based on current price position
    let (token0_amount, token1_amount) = if current_tick < tick_lower {
        // price < lower, only token0
        let token0 = get_delta_amount_0_raydium_style(sqrt_ratio_current, sqrt_ratio_upper, liquidity)?;
        (token0, 0)
    } else if current_tick >= tick_upper {
        // price > upper, only token1
        let token1 = get_delta_amount_1_raydium_style(sqrt_ratio_lower, sqrt_ratio_current, liquidity)?;
        (0, token1)
    } else {
        // price in range, both token0 and token1
        let token0 = get_delta_amount_0_raydium_style(sqrt_ratio_current, sqrt_ratio_upper, liquidity)?;
        let token1 = get_delta_amount_1_raydium_style(sqrt_ratio_lower, sqrt_ratio_current, liquidity)?;
        (token0, token1)
    };
    
    
    Ok((token0_amount, token1_amount))
}

/// Convert token amounts to USD total value
pub fn calculate_value_from_token_amounts(
    token0_amount: u64, // SOL (lamports)
    token1_amount: u64, // USDC (micro USDC)
    sol_price_usd: u64, // SOL price (8 decimal places)
    _usdc_price_usd: u64, // USDC price (6 decimal places, typically 1000000)
) -> Result<u64> {
    // SOL value calculation
    let sol_value = (token0_amount as u128)
        .checked_mul(sol_price_usd as u128)
        .ok_or(LendingError::ArithmeticOverflow)?
        .checked_div(1_000_000_000u128) // Convert lamports to SOL
        .ok_or(LendingError::ArithmeticOverflow)?;
    
    // USDC value calculation - convert to 8 decimal places format with SOL value
    // token1_amount is micro USDC, convert to 8 decimal places USD format
    // 49,083,527 micro USDC = 49.083527 USDC = 4,908,352,700 (8 decimal places)
    let usdc_value = (token1_amount as u128)
        .checked_mul(100u128) // Convert 6 decimal places to 8 decimal places
        .ok_or(LendingError::ArithmeticOverflow)?;
    
    // Total value calculation
    let total_value = sol_value
        .checked_add(usdc_value)
        .ok_or(LendingError::ArithmeticOverflow)? as u64;
    
    Ok(total_value)
}

// ============ Helper math functions ============

/// Convert tick to sqrt price (Q64.64 format) - based on Raydium CLMM standard
fn tick_to_sqrt_ratio_q64_64(tick: i32) -> Result<u128> {
    // Range check: Raydium CLMM tick range is -443636 to 443636
    require!(tick >= -443636 && tick <= 443636, LendingError::ArithmeticOverflow);
    
    // Use precise calculation of 1.0001^(tick/2) because sqrtPrice = sqrt(1.0001^tick)
    let base = 1.0001f64;
    let power = tick as f64 / 2.0; // Divide by 2 because we're calculating square root
    
    let sqrt_price = if tick >= 0 {
        base.powf(power)
    } else {
        1.0 / base.powf(-power)
    };
    
    // Convert to Q64.64 format (2^64 scaling) - this is Raydium CLMM standard
    let scale = 2f64.powi(64);
    let result = (sqrt_price * scale) as u128;
    
    Ok(result)
}

/// Raydium CLMM standard formula for token0 amount calculation
/// Δx = L * (1 / √P_lower - 1 / √P_upper) = L * (√P_upper - √P_lower) / (√P_lower * √P_upper)  
fn get_delta_amount_0_raydium_style(
    sqrt_ratio_a: u128, // Smaller sqrt price (Q64.64 format)
    sqrt_ratio_b: u128, // Larger sqrt price (Q64.64 format)
    liquidity: u128,
) -> Result<u64> {
    if sqrt_ratio_a >= sqrt_ratio_b || liquidity == 0 {
        return Ok(0);
    }
    
    
    // Δx = L * (√P_upper - √P_lower) / (√P_lower * √P_upper)
    // But to avoid overflow, we use step-by-step calculation
    
    // Since the numerator can be very large, we need to use a safe multiplication and division method
    // According to Raydium's implementation, we use mul_div way
    
    let price_diff = sqrt_ratio_b.saturating_sub(sqrt_ratio_a);
    if price_diff == 0 {
        return Ok(0);
    }
    
    // Calculate denominator: sqrt_a * sqrt_b / Q64 (reduce precision to avoid overflow)
    let denominator_raw = sqrt_ratio_a
        .checked_mul(sqrt_ratio_b)
        .ok_or(LendingError::ArithmeticOverflow)?;
    
    // Divide denominator by Q64 to reduce numerical size
    let denominator = denominator_raw
        .checked_div(1u128 << 64)
        .ok_or(LendingError::ArithmeticOverflow)?;
    
    if denominator == 0 {
        return Ok(0);
    }
    
    // Calculate: liquidity * price_diff / denominator
    let numerator = liquidity
        .checked_mul(price_diff)
        .ok_or(LendingError::ArithmeticOverflow)?;
    
    let result = numerator
        .checked_div(denominator)
        .ok_or(LendingError::ArithmeticOverflow)? as u64;
    
    Ok(result)
}

/// Raydium CLMM standard formula for token1 amount calculation
/// Δy = L * (√P_upper - √P_lower) / Q64
fn get_delta_amount_1_raydium_style(
    sqrt_ratio_a: u128, // Smaller sqrt price (Q64.64 format)
    sqrt_ratio_b: u128, // Larger sqrt price (Q64.64 format)
    liquidity: u128,
) -> Result<u64> {
    if sqrt_ratio_a >= sqrt_ratio_b || liquidity == 0 {
        return Ok(0);
    }
    
    let price_diff = sqrt_ratio_b.saturating_sub(sqrt_ratio_a);
    
    
    if price_diff == 0 {
        return Ok(0);
    }
    
    // Calculate: Δy = L * (√P_upper - √P_lower) / Q64
    // This is simpler than Token0 calculation because there's no complex denominator
    
    // Calculate: liquidity * price_diff / Q64
    let numerator = liquidity
        .checked_mul(price_diff)
        .ok_or(LendingError::ArithmeticOverflow)?;
    
    // Divide by Q64 (2^64) to reduce numerical size
    let q64 = 1u128 << 64;
    let result = numerator
        .checked_div(q64)
        .ok_or(LendingError::ArithmeticOverflow)? as u64;
    
    Ok(result)
}

/// Convert tick to sqrt price (Q64.64 format) - more precise calculation method
#[allow(dead_code)]
fn tick_to_sqrt_ratio(tick: i32) -> Result<u128> {
    // Add tick range check to prevent overflow for extreme values
    require!(tick >= -887272 && tick <= 887272, LendingError::ArithmeticOverflow);
    
    
    // Use more precise math calculation: sqrt(1.0001^tick)
    // 1.0001 = 10001/10000
    let base_ratio = if tick >= 0 {
        // For positive tick, calculate 1.0001^tick
        let mut result = 1.0f64;
        let base = 1.0001f64;
        let mut power = tick as u32;
        
        // Use fast power algorithm
        let mut temp_base = base;
        while power > 0 {
            if power & 1 == 1 {
                result *= temp_base;
            }
            temp_base *= temp_base;
            power >>= 1;
        }
        result.sqrt()
    } else {
        // For negative tick, calculate 1/(1.0001^|tick|)
        let mut result = 1.0f64;
        let base = 1.0001f64;
        let mut power = tick.abs() as u32;
        
        // Use fast power algorithm
        let mut temp_base = base;
        while power > 0 {
            if power & 1 == 1 {
                result *= temp_base;
            }
            temp_base *= temp_base;
            power >>= 1;
        }
        1.0 / result.sqrt()
    };
    
    // Ensure ratio is within valid range
    if base_ratio <= 0.0 || base_ratio.is_infinite() || base_ratio.is_nan() {
        return Err(LendingError::ArithmeticOverflow.into());
    }
    
    // Convert to Q64.96 format
    let scale = 2f64.powi(96);
    let result = base_ratio * scale;
    
    if result < 0.0 || result > (u128::MAX as f64) {
        return Err(LendingError::ArithmeticOverflow.into());
    }
    
    let final_result = result as u128;
    
    Ok(final_result)
}

/// Calculate token0 amount when price is in range
#[allow(dead_code)]
fn calculate_token0_in_range(liquidity: u128, sqrt_price_current: u128, sqrt_price_upper: u128) -> Result<u64> {
    if sqrt_price_current >= sqrt_price_upper {
        return Ok(0);
    }
    
    // Calculate: liquidity * (sqrt_price_upper - sqrt_price_current) / (sqrt_price_current * sqrt_price_upper)
    let price_diff = sqrt_price_upper.saturating_sub(sqrt_price_current);
    
    
    if price_diff == 0 {
        return Ok(0);
    }
    
    // Use step-by-step calculation to avoid overflow
    // First calculate: liquidity * price_diff
    let numerator = liquidity
        .checked_mul(price_diff)
        .ok_or(LendingError::ArithmeticOverflow)?;
        
    
    // Calculate denominator: sqrt_price_current * sqrt_price_upper
    let denominator_base = sqrt_price_current
        .checked_mul(sqrt_price_upper)
        .ok_or(LendingError::ArithmeticOverflow)?;
        
    // Divide by Q64.96 format scale factor
    let denominator = denominator_base
        .checked_div(1u128 << 96)
        .unwrap_or_else(|| {
            // If overflow, use smaller scale factor
            denominator_base.checked_div(1u128 << 64).unwrap_or(denominator_base)
        });
        
    
    if denominator == 0 {
        return Ok(0);
    }
    
    let amount = numerator
        .checked_div(denominator)
        .ok_or(LendingError::ArithmeticOverflow)? as u64;
    
    Ok(amount)
}

/// Calculate token1 amount when price is in range
#[allow(dead_code)]
fn calculate_token1_in_range(liquidity: u128, sqrt_price_current: u128, sqrt_price_lower: u128) -> Result<u64> {
    if sqrt_price_current <= sqrt_price_lower {
        return Ok(0);
    }
    
    // Calculate: liquidity * (sqrt_price_current - sqrt_price_lower) / 2^96
    let price_diff = sqrt_price_current.saturating_sub(sqrt_price_lower);
    
    
    if price_diff == 0 {
        return Ok(0);
    }
    
    // Calculate: liquidity * price_diff / 2^96
    let numerator = liquidity
        .checked_mul(price_diff)
        .ok_or(LendingError::ArithmeticOverflow)?;
        
    
    // Divide by Q64.96 format scale factor
    let amount = numerator
        .checked_div(1u128 << 96)
        .unwrap_or_else(|| {
            // If overflow, use smaller scale factor
            numerator.checked_div(1u128 << 64).unwrap_or_else(|| {
                numerator.checked_div(1u128 << 32).unwrap_or(numerator)
            })
        }) as u64;
        
    Ok(amount)
}

/// Calculate token0 amount when price is out of range
#[allow(dead_code)]
fn calculate_token0_out_of_range(liquidity: u128, sqrt_price_lower: u128, sqrt_price_upper: u128) -> Result<u64> {
    let amount = liquidity
        .checked_mul(sqrt_price_upper.saturating_sub(sqrt_price_lower))
        .ok_or(LendingError::ArithmeticOverflow)?
        .checked_div(sqrt_price_lower.checked_mul(sqrt_price_upper).ok_or(LendingError::ArithmeticOverflow)?)
        .ok_or(LendingError::ArithmeticOverflow)?
        .checked_div(1u128 << 96)
        .ok_or(LendingError::ArithmeticOverflow)? as u64;
    
    Ok(amount)
}

/// Calculate token1 amount when price is out of range
#[allow(dead_code)]
fn calculate_token1_out_of_range(liquidity: u128, sqrt_price_lower: u128, sqrt_price_upper: u128) -> Result<u64> {
    let amount = liquidity
        .checked_mul(sqrt_price_upper.saturating_sub(sqrt_price_lower))
        .ok_or(LendingError::ArithmeticOverflow)?
        .checked_div(1u128 << 96)
        .ok_or(LendingError::ArithmeticOverflow)? as u64;
    
    Ok(amount)
}

/// Determine health status based on health factor
fn determine_health_status(health_factor: u64, liquidation_threshold: u16) -> u8 {
    if health_factor <= liquidation_threshold as u64 {
        2 // Dangerous
    } else if health_factor <= (liquidation_threshold + 500) as u64 {
        1 // Warning
    } else {
        0 // Healthy
    }
}

// ============ Event Definitions ============

#[event]
pub struct CollateralValueUpdated {
    pub user: Pubkey,
    pub old_value: u64,
    pub new_value: u64,
    pub health_factor: u64,
    pub sol_price: u64,
    pub updated_at: i64,
}

#[event]
pub struct HealthCheck {
    pub user: Pubkey,
    pub health_factor: u64,
    pub status: u8, // 0=Healthy, 1=Warning, 2=Dangerous
    pub checked_at: i64,
}