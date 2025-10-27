// User related query endpoints
use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::LendingError;
use crate::utils::get_sol_price;
use super::{UserPositionInfo, LpValueBreakdown};

// ============ Basic User Queries ============

/// Query user position details
#[derive(Accounts)]
pub struct GetUserPosition<'info> {
    /// User position account
    #[account(
        constraint = user_position.owner == user.key() @ LendingError::InvalidAuthority
    )]
    pub user_position: Account<'info, UserPosition>,
    
    /// User account
    pub user: Signer<'info>,
}

/// Query user position details
pub fn get_user_position(ctx: Context<GetUserPosition>) -> Result<UserPositionInfo> {
    let position = &ctx.accounts.user_position;
    
    Ok(UserPositionInfo {
        owner: position.owner,
        lp_mint: position.lp_mint,
        lp_deposited: position.lp_deposited,
        usdc_borrowed: position.usdc_borrowed,
        collateral_value_cache: position.collateral_value_cache,
        health_factor_cache: position.health_factor_cache,
        cache_timestamp: position.last_cache_update,
        total_borrow_count: position.total_borrow_count,
        is_liquidated: position.is_liquidated,
    })
}

// ============ User Health Queries ============

/// User health information
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UserHealthInfo {
    pub collateral_value_usd: u64,
    pub debt_value_usd: u64,
    pub health_factor: u64,
    pub liquidation_threshold: u64,
    pub max_borrow_amount: u64,
    pub is_healthy: bool,
    pub lp_token_amount: u64,
    pub lp_value_breakdown: LpValueBreakdown,
}

/// Query user health factor and LP value breakdown
#[derive(Accounts)]
pub struct GetUserHealthInfo<'info> {
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
    
    /// Chainlink SOL price feed
    /// CHECK: Chainlink price feed
    pub chainlink_sol_feed: UncheckedAccount<'info>,
    
    /// Chainlink program
    /// CHECK: Chainlink program
    pub chainlink_program: UncheckedAccount<'info>,
    
    /// CLMM pool account
    /// CHECK: Raydium CLMM pool
    pub clmm_pool: UncheckedAccount<'info>,
    
    /// User NFT position account
    /// CHECK: User NFT position
    pub user_nft_position: UncheckedAccount<'info>,
    
    /// User account
    pub user: Signer<'info>,
}

/// Query user health factor and LP value breakdown
pub fn get_user_health_info(ctx: Context<GetUserHealthInfo>) -> Result<UserHealthInfo> {
    let user_position = &ctx.accounts.user_position;
    
    // If user has no LP deposited, return default values
    if user_position.lp_deposited == 0 {
        return Ok(UserHealthInfo {
            collateral_value_usd: 0,
            debt_value_usd: user_position.usdc_borrowed,
            health_factor: 0,
            liquidation_threshold: 0,
            max_borrow_amount: 0,
            is_healthy: user_position.usdc_borrowed == 0,
            lp_token_amount: 0,
            lp_value_breakdown: LpValueBreakdown {
                token0_amount: 0,
                token1_amount: 0,
                token0_value_usd: 0,
                token1_value_usd: 0,
                total_value_usd: 0,
                sol_price_usd: 0,
            },
        });
    }
    
    // Get SOL price from Chainlink feed
    let sol_price = get_sol_price(
        &ctx.accounts.chainlink_program,
        &ctx.accounts.chainlink_sol_feed,
    )?;
    
    // Parse PersonalPosition data to get token amounts
    let position_info = crate::instructions::valuation::parse_personal_position_data(
        &ctx.accounts.user_nft_position.try_borrow_data()?
    )?;
    
    let current_tick = crate::instructions::valuation::parse_clmm_current_tick(
        &ctx.accounts.clmm_pool.try_borrow_data()?
    )?;
    
    let (token0_amount, token1_amount) = crate::instructions::valuation::calculate_token_amounts_from_liquidity(
        position_info.liquidity,
        current_tick,
        position_info.tick_lower,
        position_info.tick_upper,
    )?;
    
    // Calculate total LP value in USD
    let total_value_usd = crate::instructions::valuation::calculate_value_from_token_amounts(
        token0_amount,
        token1_amount,
        sol_price,
        1_000_000 // USDC price fixed at $1 (6 decimal places)
    )?;
    
    // Calculate value of each token in USD
    let token0_value_usd = (token0_amount as u128 * sol_price as u128 / 1_000_000_000) as u64; // SOL value
    let token1_value_usd = token1_amount; // USDC value
    
    // Calculate health factor
    let health_factor = if user_position.usdc_borrowed > 0 {
        (total_value_usd as u128 * 10000 / user_position.usdc_borrowed as u128) as u64
    } else {
        u64::MAX
    };
    
    // Calculate max borrow amount (50% collateral ratio)
    let max_borrow = total_value_usd / 2;
    
    Ok(UserHealthInfo {
        collateral_value_usd: total_value_usd,
        debt_value_usd: user_position.usdc_borrowed,
        health_factor,
        liquidation_threshold: ctx.accounts.protocol_config.liquidation_threshold as u64,
        max_borrow_amount: max_borrow,
        is_healthy: health_factor > 11000, // 110%
        lp_token_amount: user_position.lp_deposited,
        lp_value_breakdown: LpValueBreakdown {
            token0_amount,
            token1_amount,
            token0_value_usd,
            token1_value_usd,
            total_value_usd,
            sol_price_usd: sol_price,
        },
    })
}

// ============ User Info Summary ============

/// Simplified user summary info (no remaining_accounts required)
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UserBasicSummary {
    pub owner: Pubkey,
    pub lp_mint: Pubkey,
    pub lp_deposited: u64,
    pub deposit_days: u64,
    pub collateral_value_usd: u64,
    pub usdc_borrowed: u64,
    pub health_factor: u64,
    pub max_borrow_capacity: u64,
    pub is_healthy: bool,
    pub is_liquidated: bool,
    pub last_update: i64,
}

/// Query user basic summary info (simplified version)
#[derive(Accounts)]
pub struct GetUserBasicSummary<'info> {
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
    
    /// User account
    pub user: Signer<'info>,
}

/// Query user basic summary info (simplified version)
pub fn get_user_basic_summary(ctx: Context<GetUserBasicSummary>) -> Result<UserBasicSummary> {
    let user_position = &ctx.accounts.user_position;
    let current_timestamp = Clock::get()?.unix_timestamp;
    
    // Calculate deposit days
    let deposit_days = if user_position.created_at > 0 {
        ((current_timestamp - user_position.created_at) / 86400) as u64
    } else {
        0
    };
    
    // Calculate max borrow capacity
    let max_borrow_capacity = if user_position.collateral_value_cache > 0 {
        user_position.collateral_value_cache * 
        ctx.accounts.protocol_config.collateral_ratio as u64 / 10000 / 100 
    } else {
        0
    };
    
    // Health factor > 120% is considered healthy
    let is_healthy = user_position.health_factor_cache > 12000; // 120%
    
    Ok(UserBasicSummary {
        owner: user_position.owner,
        lp_mint: user_position.lp_mint,
        lp_deposited: user_position.lp_deposited,
        deposit_days,
        collateral_value_usd: user_position.collateral_value_cache,
        usdc_borrowed: user_position.usdc_borrowed,
        health_factor: user_position.health_factor_cache,
        max_borrow_capacity,
        is_healthy,
        is_liquidated: user_position.is_liquidated,
        last_update: user_position.last_cache_update,
    })
}

// ============ User Comprehensive Status ============

/// Comprehensive user status summary
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UserComprehensiveStatus {
    pub owner: Pubkey,
    pub lp_mint: Pubkey,
    
    // LP Deposit Information
    pub lp_deposited: u64,                    // LP token amount
    pub deposit_timestamp: i64,               // Deposit timestamp
    pub deposit_days: u64,                    // Deposit days
    pub collateral_value_usd: u64,            // Collateral value in USD
    
    // Borrow Information  
    pub total_borrowed: u64,                  // Total borrowed amount
    pub active_borrows: u32,                  // Active borrow count
    pub total_interest_owed: u64,             // Total interest owed
    pub average_borrow_rate: u16,             // Average borrow rate
    
    // Deposit Pool Information
    pub pool_deposits: u64,                   // Pool deposit amount
    pub deposit_days_in_pool: u64,            // Deposit days in pool
    pub earned_interest_from_pool: u64,       // Earned interest from pool
    pub current_deposit_rate: u16,            // Current deposit rate
    
    // Health Information
    pub health_factor: u64,                   // Health factor
    pub max_borrow_amount: u64,               // Max borrow amount (50% collateral ratio)
    pub liquidation_risk: bool,               // Liquidation risk
    pub is_liquidated: bool,                  // Whether liquidated
    
    // Summary Information
    pub net_position_usd: i64,                // Net position (collateral - debt)
    pub total_fees_paid: u64,                 // Total fees paid
    pub account_created_at: i64,              // Account creation timestamp
}

/// Query user comprehensive status summary
#[derive(Accounts)]
pub struct GetUserComprehensiveStatus<'info> {
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
    
    /// Traditional USDC pool account
    #[account(
        seeds = [b"traditional_usdc_pool", protocol_config.usdc_mint.as_ref()],
        bump = traditional_pool.bump
    )]
    pub traditional_pool: Account<'info, TraditionalUsdcPool>,
    
    /// User account
    pub user: Signer<'info>,
}

/// Query user comprehensive status summary
pub fn get_user_comprehensive_status<'info>(
    ctx: Context<'_, '_, 'info, 'info, GetUserComprehensiveStatus<'info>>
) -> Result<UserComprehensiveStatus> {
    let user_position = &ctx.accounts.user_position;
    let current_timestamp = Clock::get()?.unix_timestamp;
    
    // Calculate deposit days
    let deposit_days = if user_position.created_at > 0 {
        ((current_timestamp - user_position.created_at) / 86400) as u64
    } else {
        0
    };
    
    // Calculate borrow information
    let mut total_borrowed = 0u64;
    let mut active_borrows = 0u32;
    let mut total_interest_owed = 0u64;
    let mut total_rates = 0u64;
    let mut total_fees_paid = 0u64;
    
    // Iterate through remaining_accounts to find borrow records
    for account_info in ctx.remaining_accounts.iter() {
        if let Ok(borrow_record) = Account::<UserBorrowRecord>::try_from(account_info) {
            if borrow_record.owner == user_position.owner && !borrow_record.is_repaid {
                total_borrowed += borrow_record.principal_amount;
                active_borrows += 1;
                total_rates += borrow_record.locked_borrow_rate as u64;
                
                // Calculate current interest owed
                if let Ok(interest) = borrow_record.calculate_current_interest() {
                    total_interest_owed += interest;
                }
                
                // Estimate paid fees (simplified calculation)
                total_fees_paid += borrow_record.total_repaid_interest / 10; // 假设10%手续费
            }
        }
    }
    
    // Calculate average borrow rate
    let average_borrow_rate = if active_borrows > 0 {
        (total_rates / active_borrows as u64) as u16
    } else {
        0
    };
    
    // Calculate deposit pool information
    let mut pool_deposits = 0u64;
    let mut deposit_days_in_pool = 0u64;
    let mut earned_interest_from_pool = 0u64;
    
    // Iterate through remaining_accounts to find deposit records
    for account_info in ctx.remaining_accounts.iter() {
        if let Ok(deposit_record) = Account::<UserDepositRecord>::try_from(account_info) {
            if deposit_record.owner == user_position.owner && !deposit_record.is_withdrawn {
                pool_deposits += deposit_record.principal_amount;
                earned_interest_from_pool += deposit_record.earned_interest;
                
                // Calculate deposit days in pool
                let days = ((current_timestamp - deposit_record.deposit_timestamp) / 86400) as u64;
                if days > deposit_days_in_pool {
                    deposit_days_in_pool = days;
                }
            }
        }
    }
    
    // Calculate health factor and risk
    let health_factor = user_position.health_factor_cache;
    let liquidation_risk = health_factor < 12000; // 120% health factor threshold
    
    // Calculate max borrow amount (based on current collateral value)
    let max_borrow_amount = if user_position.collateral_value_cache > 0 {
        let max_total = user_position.collateral_value_cache * 
                       ctx.accounts.protocol_config.collateral_ratio as u64 / 10000 / 100; 
        if max_total > user_position.usdc_borrowed {
            max_total - user_position.usdc_borrowed
        } else {
            0
        }
    } else {
        0
    };
    
    // Calculate net position value (collateral - borrowed)
    let net_position_usd = user_position.collateral_value_cache as i64 - 
                          (user_position.usdc_borrowed * 100) as i64; 
    
    Ok(UserComprehensiveStatus {
        owner: user_position.owner,
        lp_mint: user_position.lp_mint,
        
        // LP Deposit Information
        lp_deposited: user_position.lp_deposited,
        deposit_timestamp: user_position.created_at,
        deposit_days,
        collateral_value_usd: user_position.collateral_value_cache,
        
        // Borrow information
        total_borrowed: user_position.usdc_borrowed,
        active_borrows,
        total_interest_owed,
        average_borrow_rate,
        
        // Deposit Pool Information
        pool_deposits,
        deposit_days_in_pool,
        earned_interest_from_pool,
        current_deposit_rate: ctx.accounts.traditional_pool.current_deposit_rate,
        
        // Health Factor Information
        health_factor,
        max_borrow_amount,
        liquidation_risk,
        is_liquidated: user_position.is_liquidated,
        
        // Summary Information
        net_position_usd,
        total_fees_paid,
        account_created_at: user_position.created_at,
    })
}