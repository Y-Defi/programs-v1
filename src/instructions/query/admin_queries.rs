// Admin Query Interfaces
use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::LendingError;
use super::user_queries::UserBasicSummary;

// ============ Admin Query Interfaces ============

/// All Users Status Summary
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct AllUsersStatus {
    pub total_users: u32,
    pub total_collateral_value: u64,
    pub total_borrowed: u64,
    pub healthy_users: u32,
    pub at_risk_users: u32,
    pub liquidated_users: u32,
    pub users: Vec<UserBasicSummary>,
}

/// Admin Query All Users Status
#[derive(Accounts)]
pub struct GetAllUsersStatus<'info> {
    /// Protocol Configuration
    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        constraint = protocol_config.authority == authority.key() @ LendingError::InvalidAuthority
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
    
    /// Admin Authority
    pub authority: Signer<'info>,
}

/// Admin Query All Users Status
pub fn get_all_users_status<'info>(
    ctx: Context<'_, '_, 'info, 'info, GetAllUsersStatus<'info>>
) -> Result<AllUsersStatus> {
    let current_timestamp = Clock::get()?.unix_timestamp;
    let mut users = Vec::new();
    let mut total_collateral_value = 0u64;
    let mut total_borrowed = 0u64;
    let mut healthy_users = 0u32;
    let mut at_risk_users = 0u32;
    let mut liquidated_users = 0u32;
    
    // Iterate through UserPosition accounts in remaining_accounts
    for account_info in ctx.remaining_accounts.iter() {
        if let Ok(user_position) = Account::<UserPosition>::try_from(account_info) {
            // Calculate Deposit Duration in Days
            let deposit_days = if user_position.created_at > 0 {
                ((current_timestamp - user_position.created_at) / 86400) as u64
            } else {
                0
            };
            
            // Calculate Maximum Borrow Capacity
            let max_borrow_capacity = if user_position.collateral_value_cache > 0 {
                user_position.collateral_value_cache * 
                ctx.accounts.protocol_config.collateral_ratio as u64 / 10000 / 100
            } else {
                0
            };
            
            // Check Health Status
            let is_healthy = user_position.health_factor_cache > 12000;
            let is_liquidated = user_position.is_liquidated;
            
            // Update Statistics
            total_collateral_value += user_position.collateral_value_cache;
            total_borrowed += user_position.usdc_borrowed;
            
            if is_liquidated {
                liquidated_users += 1;
            } else if is_healthy {
                healthy_users += 1;
            } else {
                at_risk_users += 1;
            }
            
            let user_summary = UserBasicSummary {
                owner: user_position.owner,
                lp_mint: user_position.lp_mint,
                lp_deposited: user_position.lp_deposited,
                deposit_days,
                collateral_value_usd: user_position.collateral_value_cache,
                usdc_borrowed: user_position.usdc_borrowed,
                health_factor: user_position.health_factor_cache,
                max_borrow_capacity,
                is_healthy,
                is_liquidated,
                last_update: user_position.last_cache_update,
            };
            
            users.push(user_summary);
        }
    }
    
    Ok(AllUsersStatus {
        total_users: users.len() as u32,
        total_collateral_value,
        total_borrowed,
        healthy_users,
        at_risk_users,
        liquidated_users,
        users,
    })
}

/// Protocol Statistics Summary
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ProtocolStatistics {
    pub total_users: u32,
    pub total_lp_value_locked: u64,
    pub total_usdc_borrowed: u64,
    pub total_usdc_deposited: u64,
    pub protocol_revenue: u64,
    pub utilization_rate: u16,
    pub average_health_factor: u64,
    pub liquidations_count: u32,
}

/// Admin Query Protocol Statistics
#[derive(Accounts)]
pub struct GetProtocolStatistics<'info> {
    /// Protocol Configuration
    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        constraint = protocol_config.authority == authority.key() @ LendingError::InvalidAuthority
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
    
    /// Traditional USDC Pool Account
    #[account(
        seeds = [b"traditional_usdc_pool", protocol_config.usdc_mint.as_ref()],
        bump = traditional_pool.bump
    )]
    pub traditional_pool: Account<'info, TraditionalUsdcPool>,
    
    /// Admin Authority
    pub authority: Signer<'info>,
}

/// Admin Query Protocol Statistics
pub fn get_protocol_statistics(ctx: Context<GetProtocolStatistics>) -> Result<ProtocolStatistics> {
    let protocol_config = &ctx.accounts.protocol_config;
    let traditional_pool = &ctx.accounts.traditional_pool;

    // Calculate Utilization Rate
    let utilization_rate = if traditional_pool.total_deposits > 0 {
        ((traditional_pool.total_borrowed as u128 * 10000) / traditional_pool.total_deposits as u128) as u16
    } else {
        0
    };
    
    // Calculate Protocol Revenue (simplified estimation)
    let protocol_revenue = traditional_pool.total_borrowed / 100; 
    
    Ok(ProtocolStatistics {
        total_users: protocol_config.total_users,
        total_lp_value_locked: protocol_config.total_collateral_value,
        total_usdc_borrowed: protocol_config.total_borrowed,
        total_usdc_deposited: traditional_pool.total_deposits,
        protocol_revenue,
        utilization_rate,
        average_health_factor: 15000, // need to calculate from user positions
        liquidations_count: 0, // need to calculate from events
    })
}

/// Admin Query Risk Report
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct RiskReport {
    pub total_positions: u32,
    pub healthy_positions: u32,
    pub warning_positions: u32,      // 120-130%
    pub danger_positions: u32,       // 110-120%
    pub critical_positions: u32,     // <110%
    pub liquidated_positions: u32,
    pub total_at_risk_value: u64,
    pub largest_position_value: u64,
    pub average_health_factor: u64,
}

/// Admin Query Risk Report
#[derive(Accounts)]
pub struct GenerateRiskReport<'info> {
    /// Protocol Configuration
    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        constraint = protocol_config.authority == authority.key() @ LendingError::InvalidAuthority
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
    
    /// Admin Authority
    pub authority: Signer<'info>,
}

/// Admin Query Risk Report
pub fn generate_risk_report<'info>(
    ctx: Context<'_, '_, 'info, 'info, GenerateRiskReport<'info>>
) -> Result<RiskReport> {
    let mut total_positions = 0u32;
    let mut healthy_positions = 0u32;
    let mut warning_positions = 0u32;
    let mut danger_positions = 0u32;
    let mut critical_positions = 0u32;
    let mut liquidated_positions = 0u32;
    let mut total_at_risk_value = 0u64;
    let mut largest_position_value = 0u64;
    let mut total_health_factor = 0u128;
    let mut active_positions = 0u32;
    
    // Iterate through remaining_accounts to collect user positions
    for account_info in ctx.remaining_accounts.iter() {
        if let Ok(user_position) = Account::<UserPosition>::try_from(account_info) {
            total_positions += 1;
            
            if user_position.is_liquidated {
                liquidated_positions += 1;
                continue;
            }
            
            if user_position.usdc_borrowed == 0 {
                healthy_positions += 1;
                continue;
            }
            
            active_positions += 1;
            let health_factor = user_position.health_factor_cache;
            total_health_factor += health_factor as u128;
            
            // Update largest position value
            if user_position.collateral_value_cache > largest_position_value {
                largest_position_value = user_position.collateral_value_cache;
            }
            
            // Classify positions by health factor
            if health_factor >= 13000 {
                healthy_positions += 1;
            } else if health_factor >= 12000 {
                warning_positions += 1;
                total_at_risk_value += user_position.collateral_value_cache;
            } else if health_factor >= 11000 {
                danger_positions += 1;
                total_at_risk_value += user_position.collateral_value_cache;
            } else {
                critical_positions += 1;
                total_at_risk_value += user_position.collateral_value_cache;
            }
        }
    }
    
    let average_health_factor = if active_positions > 0 {
        (total_health_factor / active_positions as u128) as u64
    } else {
        u64::MAX
    };
    
    Ok(RiskReport {
        total_positions,
        healthy_positions,
        warning_positions,
        danger_positions,
        critical_positions,
        liquidated_positions,
        total_at_risk_value,
        largest_position_value,
        average_health_factor,
    })
}