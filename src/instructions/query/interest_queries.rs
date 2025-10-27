// Interest Calculation Related Query Interfaces
use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::LendingError;
use super::BorrowRecordInfo;

// ============ Interest Calculation Queries ============

/// Query User Borrow Record
#[derive(Accounts)]
pub struct GetUserBorrowRecord<'info> {
    /// Borrow Record Account
    #[account(
        constraint = borrow_record.owner == user.key() @ LendingError::InvalidAuthority
    )]
    pub borrow_record: Account<'info, UserBorrowRecord>,
    
    /// User Account
    pub user: Signer<'info>,
}

/// Query Single Borrow Record Information
pub fn get_user_borrow_record(ctx: Context<GetUserBorrowRecord>) -> Result<BorrowRecordInfo> {
    let record = &ctx.accounts.borrow_record;
    
    Ok(BorrowRecordInfo {
        record_id: record.borrow_index,
        owner: record.owner,
        pool: record.pool,
        principal_amount: record.principal_amount,
        locked_borrow_rate: record.locked_borrow_rate,
        borrow_timestamp: record.borrow_timestamp,
        is_repaid: record.is_repaid,
        total_repaid: record.total_repaid_principal,
        last_repay_timestamp: record.last_repay_timestamp,
    })
}

/// Calculate User Current Interest Owed
#[derive(Accounts)]
pub struct CalculateUserInterest<'info> {
    /// Borrow Record Account
    #[account(
        constraint = borrow_record.owner == user.key() @ LendingError::InvalidAuthority,
        constraint = !borrow_record.is_repaid @ LendingError::UserPositionNotFound
    )]
    pub borrow_record: Account<'info, UserBorrowRecord>,
    
    /// User Account
    pub user: Signer<'info>,
}

/// Calculate User Current Interest Owed
pub fn calculate_user_interest(ctx: Context<CalculateUserInterest>) -> Result<u64> {
    let record = &ctx.accounts.borrow_record;
    let current_timestamp = Clock::get()?.unix_timestamp;
    
    let interest = TraditionalUsdcPool::calculate_borrow_interest(
        record.remaining_principal,
        record.locked_borrow_rate,
        record.borrow_timestamp,
    )?;
    
    
    Ok(interest)
}

// ============ Interest Details ============

/// Interest Details
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct InterestDetails {
    pub principal_amount: u64,
    pub locked_rate: u16,
    pub borrow_duration_days: u64,
    pub accrued_interest: u64,
    pub daily_interest: u64,
    pub total_owed: u64,
    pub is_overdue: bool,
}

/// Query Borrow Interest Details
#[derive(Accounts)]
pub struct GetBorrowInterestDetails<'info> {
    /// Borrow Record Account
    #[account(
        constraint = borrow_record.owner == user.key() @ LendingError::InvalidAuthority,
        constraint = !borrow_record.is_repaid @ LendingError::UserPositionNotFound
    )]
    pub borrow_record: Account<'info, UserBorrowRecord>,
    
    /// User Account
    pub user: Signer<'info>,
}

/// Query Borrow Interest Details
pub fn get_borrow_interest_details(ctx: Context<GetBorrowInterestDetails>) -> Result<InterestDetails> {
    let record = &ctx.accounts.borrow_record;
    let current_timestamp = Clock::get()?.unix_timestamp;
    
    // Calculate Borrow Duration in Days
    let borrow_duration_days = ((current_timestamp - record.borrow_timestamp) / 86400) as u64;
    
    // Calculate Accrued Interest
    let accrued_interest = record.calculate_current_interest()?;
    
    // Calculate Daily Interest
    let daily_interest = if borrow_duration_days > 0 {
        accrued_interest / borrow_duration_days
    } else {
        0
    };
    
    // Calculate Total Owed
    let total_owed = record.remaining_principal + accrued_interest;
    
    // Check if Overdue (assuming 90 days as overdue standard)
    let is_overdue = borrow_duration_days > 90;
    
    Ok(InterestDetails {
        principal_amount: record.principal_amount,
        locked_rate: record.locked_borrow_rate,
        borrow_duration_days,
        accrued_interest,
        daily_interest,
        total_owed,
        is_overdue,
    })
}

// ============ User Interest Summary ============

/// User Interest Summary
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UserInterestSummary {
    pub owner: Pubkey,
    pub total_principal: u64,
    pub total_accrued_interest: u64,
    pub total_owed: u64,
    pub active_borrows: u32,
    pub average_rate: u16,
    pub oldest_borrow_days: u64,
    pub total_daily_interest: u64,
    pub estimated_monthly_interest: u64,
}

/// Query User Interest Summary
#[derive(Accounts)]
pub struct GetUserInterestSummary<'info> {
    /// User Account
    pub user: Signer<'info>,
}

/// Get User Interest Summary
pub fn get_user_interest_summary<'info>(
    ctx: Context<'_, '_, 'info, 'info, GetUserInterestSummary<'info>>
) -> Result<UserInterestSummary> {
    let user_key = ctx.accounts.user.key();
    let current_timestamp = Clock::get()?.unix_timestamp;
    
    let mut total_principal = 0u64;
    let mut total_accrued_interest = 0u64;
    let mut active_borrows = 0u32;
    let mut total_rates = 0u64;
    let mut oldest_borrow_timestamp = current_timestamp;
    let mut total_daily_interest = 0u64;
    
    // go through all remaining accounts to find borrow records
    for account_info in ctx.remaining_accounts.iter() {
        if let Ok(borrow_record) = Account::<UserBorrowRecord>::try_from(account_info) {
            if borrow_record.owner == user_key && !borrow_record.is_repaid {
                total_principal += borrow_record.principal_amount;
                active_borrows += 1;
                total_rates += borrow_record.locked_borrow_rate as u64;
                
                // Calculate Accrued Interest
                if let Ok(interest) = borrow_record.calculate_current_interest() {
                    total_accrued_interest += interest;
                    
                    // Calculate Daily Interest
                    let days = ((current_timestamp - borrow_record.borrow_timestamp) / 86400) as u64;
                    if days > 0 {
                        total_daily_interest += interest / days;
                    }
                }
                
                // Find Oldest Borrow
                if borrow_record.borrow_timestamp < oldest_borrow_timestamp {
                    oldest_borrow_timestamp = borrow_record.borrow_timestamp;
                }
            }
        }
    }
    
    let average_rate = if active_borrows > 0 {
        (total_rates / active_borrows as u64) as u16
    } else {
        0
    };
    
    let oldest_borrow_days = if oldest_borrow_timestamp < current_timestamp {
        ((current_timestamp - oldest_borrow_timestamp) / 86400) as u64
    } else {
        0
    };
    
    let estimated_monthly_interest = total_daily_interest * 30;
    let total_owed = total_principal + total_accrued_interest;
    
    Ok(UserInterestSummary {
        owner: user_key,
        total_principal,
        total_accrued_interest,
        total_owed,
        active_borrows,
        average_rate,
        oldest_borrow_days,
        total_daily_interest,
        estimated_monthly_interest,
    })
}

// ============ Deposit Interest Queries ============

/// Deposit Interest Details
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct DepositInterestDetails {
    pub principal_amount: u64,
    pub current_rate: u16,
    pub deposit_duration_days: u64,
    pub earned_interest: u64,
    pub daily_earnings: u64,
    pub projected_monthly_earnings: u64,
    pub total_value: u64,
}

/// Query Deposit Interest Details
#[derive(Accounts)]
pub struct GetDepositInterestDetails<'info> {
    /// Deposit Record Account
    #[account(
        constraint = deposit_record.owner == user.key() @ LendingError::InvalidAuthority,
        constraint = !deposit_record.is_withdrawn @ LendingError::UserPositionNotFound
    )]
    pub deposit_record: Account<'info, UserDepositRecord>,
    
    /// Traditional USDC Pool Account (to get current rate)
    #[account(
        constraint = traditional_pool.key() == deposit_record.pool @ LendingError::InvalidAuthority
    )]
    pub traditional_pool: Account<'info, TraditionalUsdcPool>,
    
    /// User Account
    pub user: Signer<'info>,
}

/// Query Deposit Interest Details
pub fn get_deposit_interest_details(ctx: Context<GetDepositInterestDetails>) -> Result<DepositInterestDetails> {
    let record = &ctx.accounts.deposit_record;
    let pool = &ctx.accounts.traditional_pool;
    let current_timestamp = Clock::get()?.unix_timestamp;
    
    // Calculate Deposit Duration Days
    let deposit_duration_days = ((current_timestamp - record.deposit_timestamp) / 86400) as u64;
    
    // Use Current Deposit Rate to Estimate Interest Earned
    let daily_earnings = if deposit_duration_days > 0 {
        record.earned_interest / deposit_duration_days
    } else {
        // Estimate Daily Interest Based on Current Rate
        record.principal_amount * pool.current_deposit_rate as u64 / 10000 / 365
    };
    
    let projected_monthly_earnings = daily_earnings * 30;
    let total_value = record.principal_amount + record.earned_interest;
    
    Ok(DepositInterestDetails {
        principal_amount: record.principal_amount,
        current_rate: pool.current_deposit_rate,
        deposit_duration_days,
        earned_interest: record.earned_interest,
        daily_earnings,
        projected_monthly_earnings,
        total_value,
    })
}