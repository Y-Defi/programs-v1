// Pool Queries
use anchor_lang::prelude::*;
use crate::state::*;

// ============ Query Protocol Status ============

/// Protocol Status Information
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ProtocolStatusInfo {
    pub admin: Pubkey,
    pub usdc_mint: Pubkey,
    pub total_deposited: u64,
    pub total_borrowed: u64,
    pub total_collateral_value: u64,
    pub collateral_ratio: u16,
    pub liquidation_threshold: u16,
    pub is_paused: bool,
    pub total_users: u32,
}

/// Query Protocol Status
#[derive(Accounts)]
pub struct GetProtocolStatus<'info> {
    /// Protocol Configuration Account
    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
}

/// Query Overall Protocol Status
pub fn get_protocol_status(ctx: Context<GetProtocolStatus>) -> Result<ProtocolStatusInfo> {
    let config = &ctx.accounts.protocol_config;
    
    Ok(ProtocolStatusInfo {
        admin: config.authority,
        usdc_mint: config.usdc_mint,
        total_deposited: config.total_deposited,
        total_borrowed: config.total_borrowed,
        total_collateral_value: config.total_collateral_value,
        collateral_ratio: config.collateral_ratio,
        liquidation_threshold: config.liquidation_threshold,
        is_paused: config.is_paused,
        total_users: config.total_users,
    })
}

// ============ Query Pool Status ============

/// Pool Status Information
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PoolStatusInfo {
    pub pool_address: Pubkey,
    pub usdc_mint: Pubkey,
    pub vault_address: Pubkey,
    pub total_deposits: u64,
    pub total_borrows: u64,
    pub available_liquidity: u64,
    pub utilization_rate: u16,
    pub current_deposit_rate: u16,
    pub current_borrow_rate: u16,
    pub last_update_timestamp: i64,
    pub min_deposit_amount: u64,
}

/// Query Traditional USDC Pool Status
#[derive(Accounts)]
pub struct GetPoolStatus<'info> {
    /// Traditional USDC Pool Account
    #[account(
        seeds = [b"traditional_usdc_pool", pool.usdc_mint.as_ref()],
        bump = pool.bump
    )]
    pub pool: Account<'info, TraditionalUsdcPool>,
    
    /// Pool Vault Account
    #[account(
        seeds = [b"traditional_usdc_vault", pool.key().as_ref()],
        bump
    )]
    pub pool_vault: Account<'info, anchor_spl::token::TokenAccount>,
}

/// Query Traditional USDC Pool Status
pub fn get_pool_status(ctx: Context<GetPoolStatus>) -> Result<PoolStatusInfo> {
    let pool = &ctx.accounts.pool;
    let vault = &ctx.accounts.pool_vault;
    
    // Calculate utilization rate
    let utilization_rate = if pool.total_deposits > 0 {
        ((pool.total_borrowed as u128 * 10000) / pool.total_deposits as u128) as u16
    } else {
        0
    };
    
    Ok(PoolStatusInfo {
        pool_address: pool.key(),
        usdc_mint: pool.usdc_mint,
        vault_address: vault.key(),
        total_deposits: pool.total_deposits,
        total_borrows: pool.total_borrowed,
        available_liquidity: vault.amount,
        utilization_rate,
        current_deposit_rate: pool.current_deposit_rate,
        current_borrow_rate: pool.current_borrow_rate,
        last_update_timestamp: pool.last_rate_update,
        min_deposit_amount: pool.min_deposit_amount,
    })
}

// ============ Query Current Rates ============

/// Current Rates Information
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RateInfo {
    pub base_deposit_rate: u16,
    pub base_borrow_rate: u16,
    pub current_deposit_rate: u16,
    pub current_borrow_rate: u16,
    pub utilization_rate: u16,
    pub rate_update_timestamp: i64,
}

/// Query Current Rates
#[derive(Accounts)]
pub struct GetCurrentRates<'info> {
    /// Traditional USDC Pool Account
    pub pool: Account<'info, TraditionalUsdcPool>,
}

/// Query Current Rates
pub fn get_current_rates(ctx: Context<GetCurrentRates>) -> Result<RateInfo> {
    let pool = &ctx.accounts.pool;
    
    // Calculate utilization rate
    let utilization_rate = if pool.total_deposits > 0 {
        ((pool.total_borrowed as u128 * 10000) / pool.total_deposits as u128) as u16
    } else {
        0
    };
    
    Ok(RateInfo {
        base_deposit_rate: pool.base_deposit_rate,
        base_borrow_rate: pool.base_borrow_rate,
        current_deposit_rate: pool.current_deposit_rate,
        current_borrow_rate: pool.current_borrow_rate,
        utilization_rate,
        rate_update_timestamp: pool.last_rate_update,
    })
}

// ============ Query Pool Detailed Stats ============

/// Pool Detailed Statistics
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct PoolDetailedStats {
    pub pool_address: Pubkey,
    pub total_deposits: u64,
    pub total_borrows: u64,
    pub available_liquidity: u64,
    pub utilization_rate: u16,
    
    // Rate Information
    pub current_deposit_rate: u16,
    pub current_borrow_rate: u16,
    pub rate_spread: u16,
    
    // Deposit Statistics
    pub total_depositors: u32,
    pub largest_deposit: u64,
    pub average_deposit: u64,
    
    // Borrow Statistics
    pub total_borrowers: u32,
    pub largest_borrow: u64,
    pub average_borrow: u64,
    
    // Time Information
    pub pool_created_at: i64,
    pub last_activity: i64,
    pub total_volume: u64,
}

/// Query Pool Detailed Statistics
#[derive(Accounts)]
pub struct GetPoolDetailedStats<'info> {
    /// Traditional USDC Pool Account
    #[account(
        seeds = [b"traditional_usdc_pool", pool.usdc_mint.as_ref()],
        bump = pool.bump
    )]
    pub pool: Account<'info, TraditionalUsdcPool>,
    
    /// Pool Vault Account
    #[account(
        seeds = [b"traditional_usdc_vault", pool.key().as_ref()],
        bump
    )]
    pub pool_vault: Account<'info, anchor_spl::token::TokenAccount>,
}

/// Query Pool Detailed Statistics
pub fn get_pool_detailed_stats<'info>(
    ctx: Context<'_, '_, 'info, 'info, GetPoolDetailedStats<'info>>
) -> Result<PoolDetailedStats> {
    let pool = &ctx.accounts.pool;
    let vault = &ctx.accounts.pool_vault;
    
    // Calculate utilization rate
    let utilization_rate = if pool.total_deposits > 0 {
        ((pool.total_borrowed as u128 * 10000) / pool.total_deposits as u128) as u16
    } else {
        0
    };
    
    let rate_spread = pool.current_borrow_rate.saturating_sub(pool.current_deposit_rate);
    
    // Statistics for deposits and borrows (need to iterate through remaining_accounts)
    let mut total_depositors = 0u32;
    let mut total_borrowers = 0u32;
    let mut largest_deposit = 0u64;
    let mut largest_borrow = 0u64;
    let mut total_deposit_count = 0u64;
    let mut total_borrow_count = 0u64;
    
    for account_info in ctx.remaining_accounts.iter() {
        // Try to parse deposit record
        if let Ok(deposit_record) = Account::<UserDepositRecord>::try_from(account_info) {
            if !deposit_record.is_withdrawn {
                total_depositors += 1;
                total_deposit_count += deposit_record.principal_amount;
                if deposit_record.principal_amount > largest_deposit {
                    largest_deposit = deposit_record.principal_amount;
                }
            }
        }
        
        // Try to parse borrow record
        if let Ok(borrow_record) = Account::<UserBorrowRecord>::try_from(account_info) {
            if !borrow_record.is_repaid {
                total_borrowers += 1;
                total_borrow_count += borrow_record.principal_amount;
                if borrow_record.principal_amount > largest_borrow {
                    largest_borrow = borrow_record.principal_amount;
                }
            }
        }
    }
    
    let average_deposit = if total_depositors > 0 {
        total_deposit_count / total_depositors as u64
    } else {
        0
    };
    
    let average_borrow = if total_borrowers > 0 {
        total_borrow_count / total_borrowers as u64
    } else {
        0
    };
    
    Ok(PoolDetailedStats {
        pool_address: pool.key(),
        total_deposits: pool.total_deposits,
        total_borrows: pool.total_borrowed,
        available_liquidity: vault.amount,
        utilization_rate,
        
        current_deposit_rate: pool.current_deposit_rate,
        current_borrow_rate: pool.current_borrow_rate,
        rate_spread,
        
        total_depositors,
        largest_deposit,
        average_deposit,
        
        total_borrowers,
        largest_borrow,
        average_borrow,
        
        pool_created_at: pool.created_at,
        last_activity: pool.last_rate_update,
        total_volume: pool.total_deposits + pool.total_borrowed, 
    })
}