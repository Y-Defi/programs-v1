// Batch Query Interfaces
use anchor_lang::prelude::*;
use crate::state::*;
use super::{BorrowRecordInfo, DepositRecordInfo};

// ============ Batch Borrow Record Queries ============

/// Batch Borrow Record Information
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UserBorrowRecords {
    pub owner: Pubkey,
    pub total_records: u32,
    pub active_borrows: u32,
    pub total_principal: u64,
    pub total_interest_owed: u64,
    pub records: Vec<BorrowRecordInfo>,
}

/// Query All Borrow Records of a User
#[derive(Accounts)]
pub struct GetUserAllBorrowRecords<'info> {
    /// User Account
    pub user: Signer<'info>,
}

/// Batch Query All Borrow Records of a User
/// Note: This function returns summary information, specific records need to be passed via remaining_accounts
pub fn get_user_all_borrow_records<'info>(ctx: Context<'_, '_, 'info, 'info, GetUserAllBorrowRecords<'info>>) -> Result<UserBorrowRecords> {
    let user_key = ctx.accounts.user.key();
    let mut records = Vec::new();
    let mut total_principal = 0u64;
    let mut total_interest_owed = 0u64;
    let mut active_borrows = 0u32;
    
    // Go through all borrow record accounts in remaining_accounts
    for account_info in ctx.remaining_accounts.iter() {
        // Try to parse as UserBorrowRecord
        if let Ok(borrow_record) = Account::<UserBorrowRecord>::try_from(account_info) {
            // Verify it belongs to current user
            if borrow_record.owner == user_key {
                let record_info = BorrowRecordInfo {
                    record_id: borrow_record.borrow_index,
                    owner: borrow_record.owner,
                    pool: borrow_record.pool,
                    principal_amount: borrow_record.principal_amount,
                    locked_borrow_rate: borrow_record.locked_borrow_rate,
                    borrow_timestamp: borrow_record.borrow_timestamp,
                    is_repaid: borrow_record.is_repaid,
                    total_repaid: borrow_record.total_repaid_principal,
                    last_repay_timestamp: borrow_record.last_repay_timestamp,
                };
                
                total_principal += borrow_record.principal_amount;
                
                if !borrow_record.is_repaid {
                    active_borrows += 1;
                    // Calculate current interest owed
                    if let Ok(interest) = borrow_record.calculate_current_interest() {
                        total_interest_owed += interest;
                    }
                }
                
                records.push(record_info);
            }
        }
    }
    
    Ok(UserBorrowRecords {
        owner: user_key,
        total_records: records.len() as u32,
        active_borrows,
        total_principal,
        total_interest_owed,
        records,
    })
}

// ============ Batch Deposit Record Queries ============

/// Batch Deposit Record Information
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UserDepositRecords {
    pub owner: Pubkey,
    pub total_records: u32,
    pub active_deposits: u32,
    pub total_principal: u64,
    pub total_interest_earned: u64,
    pub records: Vec<DepositRecordInfo>,
}

/// Query All Deposit Records of a User
#[derive(Accounts)]
pub struct GetUserAllDepositRecords<'info> {
    /// User Account
    pub user: Signer<'info>,
}

/// Batch Query All Deposit Records of a User
pub fn get_user_all_deposit_records<'info>(ctx: Context<'_, '_, 'info, 'info, GetUserAllDepositRecords<'info>>) -> Result<UserDepositRecords> {
    let user_key = ctx.accounts.user.key();
    let mut records = Vec::new();
    let mut total_principal = 0u64;
    let mut total_interest_earned = 0u64;
    let mut active_deposits = 0u32;
    
    // Go through all deposit record accounts in remaining_accounts
    for account_info in ctx.remaining_accounts.iter() {
        // Try to parse as UserDepositRecord
        if let Ok(deposit_record) = Account::<UserDepositRecord>::try_from(account_info) {
            // Verify it belongs to current user
            if deposit_record.owner == user_key {
                let record_info = DepositRecordInfo {
                    deposit_id: deposit_record.deposit_index,
                    owner: deposit_record.owner,
                    pool: deposit_record.pool,
                    principal_amount: deposit_record.principal_amount,
                    deposit_timestamp: deposit_record.deposit_timestamp,
                    is_withdrawn: deposit_record.is_withdrawn,
                    earned_interest: deposit_record.earned_interest,
                    withdraw_timestamp: deposit_record.withdraw_timestamp,
                };
                
                total_principal += deposit_record.principal_amount;
                total_interest_earned += deposit_record.earned_interest;
                
                if !deposit_record.is_withdrawn {
                    active_deposits += 1;
                }
                
                records.push(record_info);
            }
        }
    }
    
    Ok(UserDepositRecords {
        owner: user_key,
        total_records: records.len() as u32,
        active_deposits,
        total_principal,
        total_interest_earned,
        records,
    })
}

// ============ Batch Active Users Queries ============

/// Active Users Summary
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct ActiveUsersSummary {
    pub total_users: u32,
    pub users_with_deposits: u32,
    pub users_with_borrows: u32,
    pub users_with_both: u32,
    pub total_lp_deposited: u64,
    pub total_usdc_borrowed: u64,
    pub total_pool_deposits: u64,
    pub average_health_factor: u64,
}

/// Batch Query Active Users Summary
#[derive(Accounts)]
pub struct GetActiveUsersSummary<'info> {
    /// Protocol Config Account (Anyone can query this summary)
    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
}

/// Get Active Users Summary
pub fn get_active_users_summary<'info>(
    ctx: Context<'_, '_, 'info, 'info, GetActiveUsersSummary<'info>>
) -> Result<ActiveUsersSummary> {
    let mut total_users = 0u32;
    let mut users_with_deposits = 0u32;
    let mut users_with_borrows = 0u32;
    let mut users_with_both = 0u32;
    let mut total_lp_deposited = 0u64;
    let mut total_usdc_borrowed = 0u64;
    let mut total_pool_deposits = 0u64;
    let mut total_health_factor = 0u128;
    let mut users_with_health_factor = 0u32;
    
    // Go through all user position accounts in remaining_accounts
    for account_info in ctx.remaining_accounts.iter() {
        if let Ok(user_position) = Account::<UserPosition>::try_from(account_info) {
            total_users += 1;
            
            let has_lp_deposits = user_position.lp_deposited > 0;
            let has_borrows = user_position.usdc_borrowed > 0;
            
            if has_lp_deposits {
                users_with_deposits += 1;
                total_lp_deposited += user_position.lp_deposited;
            }
            
            if has_borrows {
                users_with_borrows += 1;
                total_usdc_borrowed += user_position.usdc_borrowed;
                total_health_factor += user_position.health_factor_cache as u128;
                users_with_health_factor += 1;
            }
            
            if has_lp_deposits && has_borrows {
                users_with_both += 1;
            }
        }
        
        // Go through all deposit record accounts in remaining_accounts
        if let Ok(deposit_record) = Account::<UserDepositRecord>::try_from(account_info) {
            if !deposit_record.is_withdrawn {
                total_pool_deposits += deposit_record.principal_amount;
            }
        }
    }
    
    let average_health_factor = if users_with_health_factor > 0 {
        (total_health_factor / users_with_health_factor as u128) as u64
    } else {
        u64::MAX
    };
    
    Ok(ActiveUsersSummary {
        total_users,
        users_with_deposits,
        users_with_borrows,
        users_with_both,
        total_lp_deposited,
        total_usdc_borrowed,
        total_pool_deposits,
        average_health_factor,
    })
}

// ============ Time Range Queries ============

/// Time Range Activity Summary
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct TimeRangeActivity {
    pub start_timestamp: i64,
    pub end_timestamp: i64,
    pub new_deposits: u32,
    pub new_borrows: u32,
    pub total_deposit_volume: u64,
    pub total_borrow_volume: u64,
    pub repayments: u32,
    pub withdrawals: u32,
    pub new_users: u32,
}

/// Query Time Range Activity
#[derive(Accounts)]
pub struct GetTimeRangeActivity<'info> {
    /// Protocol Config Account
    #[account(
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
}

/// Get Time Range Activity Summary
pub fn get_time_range_activity<'info>(
    ctx: Context<'_, '_, 'info, 'info, GetTimeRangeActivity<'info>>,
    start_timestamp: i64,
    end_timestamp: i64,
) -> Result<TimeRangeActivity> {
    let mut new_deposits = 0u32;
    let mut new_borrows = 0u32;
    let mut total_deposit_volume = 0u64;
    let mut total_borrow_volume = 0u64;
    let mut repayments = 0u32;
    let mut withdrawals = 0u32;
    let mut new_users = 0u32;
    
    // Go through all accounts in remaining_accounts to find activity within time range
    for account_info in ctx.remaining_accounts.iter() {
        // Check deposit records
        if let Ok(deposit_record) = Account::<UserDepositRecord>::try_from(account_info) {
            if deposit_record.deposit_timestamp >= start_timestamp && 
               deposit_record.deposit_timestamp <= end_timestamp {
                new_deposits += 1;
                total_deposit_volume += deposit_record.principal_amount;
            }
            
            if deposit_record.is_withdrawn && 
               deposit_record.withdraw_timestamp >= start_timestamp && 
               deposit_record.withdraw_timestamp <= end_timestamp {
                withdrawals += 1;
            }
        }
        
        // Check borrow records
        if let Ok(borrow_record) = Account::<UserBorrowRecord>::try_from(account_info) {
            if borrow_record.borrow_timestamp >= start_timestamp && 
               borrow_record.borrow_timestamp <= end_timestamp {
                new_borrows += 1;
                total_borrow_volume += borrow_record.principal_amount;
            }
            
            if borrow_record.is_repaid && 
               borrow_record.last_repay_timestamp >= start_timestamp && 
               borrow_record.last_repay_timestamp <= end_timestamp {
                repayments += 1;
            }
        }
        
        // Check new users (via UserPosition creation time)
        if let Ok(user_position) = Account::<UserPosition>::try_from(account_info) {
            if user_position.created_at >= start_timestamp && 
               user_position.created_at <= end_timestamp {
                new_users += 1;
            }
        }
    }
    
    Ok(TimeRangeActivity {
        start_timestamp,
        end_timestamp,
        new_deposits,
        new_borrows,
        total_deposit_volume,
        total_borrow_volume,
        repayments,
        withdrawals,
        new_users,
    })
}