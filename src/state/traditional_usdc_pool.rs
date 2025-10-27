
use anchor_lang::prelude::*;
use crate::errors::LendingError;
use super::constants;

/// traditional usdc pool state - not using share model
#[account]
pub struct TraditionalUsdcPool {
    /// authority of the pool
    pub authority: Pubkey,
    /// USDC mint address
    pub usdc_mint: Pubkey,
    /// pool usdc vault address
    pub pool_usdc_vault: Pubkey,
    /// total deposits amount (sum of all user deposits)
    pub total_deposits: u64,
    /// available liquidity (amount that can be borrowed)
    pub available_liquidity: u64,
    /// total borrowed amount
    pub total_borrowed: u64,
    /// base deposit rate (annual, basis points, e.g. 300 = 3%)
    pub base_deposit_rate: u16,
    /// base borrow rate (annual, basis points, e.g. 500 = 5%)
    pub base_borrow_rate: u16,
    /// current deposit rate (annual, basis points, e.g. 500 = 5%)
    pub current_deposit_rate: u16,
    /// current borrow rate (annual, basis points, e.g. 700 = 7%)
    pub current_borrow_rate: u16,
    /// last rate update timestamp
    pub last_rate_update: i64,
    /// pool creation timestamp
    pub created_at: i64,
    /// whether the pool is paused
    pub is_paused: bool,
    /// minimum deposit amount (to avoid junk data)
    pub min_deposit_amount: u64,
    /// minimum borrow amount
    pub min_borrow_amount: u64,
    /// PDA bump
    pub bump: u8,
}

impl TraditionalUsdcPool {
    /// account space size
    pub const LEN: usize = 8 + // discriminator
        32 + // authority
        32 + // usdc_mint
        32 + // pool_usdc_vault
        8 +  // total_deposits
        8 +  // available_liquidity
        8 +  // total_borrowed
        2 +  // base_deposit_rate
        2 +  // base_borrow_rate
        2 +  // current_deposit_rate
        2 +  // current_borrow_rate
        8 +  // last_rate_update
        8 +  // created_at
        1 +  // is_paused
        8 +  // min_deposit_amount
        8 +  // min_borrow_amount
        1;   // bump

    /// initialize traditional usdc pool
    pub fn initialize(
        &mut self,
        authority: Pubkey,
        usdc_mint: Pubkey,
        pool_usdc_vault: Pubkey,
        initial_deposit_rate: u16,  // e.g. 500 = 5%
        initial_borrow_rate: u16,   // e.g. 700 = 7%
        min_deposit_amount: u64,    // e.g. 1_000_000 = 1 USDC
        bump: u8,
    ) {
        self.authority = authority;
        self.usdc_mint = usdc_mint;
        self.pool_usdc_vault = pool_usdc_vault;
        self.total_deposits = 0;
        self.available_liquidity = 0;
        self.total_borrowed = 0;
        self.base_deposit_rate = initial_deposit_rate;
        self.base_borrow_rate = initial_borrow_rate;
        self.current_deposit_rate = initial_deposit_rate;
        self.current_borrow_rate = initial_borrow_rate;
        self.last_rate_update = Clock::get().unwrap().unix_timestamp;
        self.created_at = Clock::get().unwrap().unix_timestamp;
        self.is_paused = false;
        self.min_deposit_amount = min_deposit_amount;
        self.min_borrow_amount = min_deposit_amount; // borrow min amount same as deposit min amount
        self.bump = bump;
    }

    /// handle deposit - record principal, not using share model
    pub fn deposit(&mut self, amount: u64) -> Result<()> {
        require!(amount >= self.min_deposit_amount, LendingError::BelowMinimumDeposit);
        require!(!self.is_paused, LendingError::PoolPaused);

        self.total_deposits = self.total_deposits
            .checked_add(amount)
            .ok_or(LendingError::ArithmeticOverflow)?;
        
        self.available_liquidity = self.available_liquidity
            .checked_add(amount)
            .ok_or(LendingError::ArithmeticOverflow)?;

        Ok(())
    }

    /// handle withdraw - calculate interest based on current rate
    pub fn withdraw(&mut self, principal: u64, interest: u64) -> Result<()> {
        let total_amount = principal
            .checked_add(interest)
            .ok_or(LendingError::ArithmeticOverflow)?;
        
        require!(
            self.available_liquidity >= total_amount,
            LendingError::InsufficientLiquidity
        );

        self.total_deposits = self.total_deposits
            .checked_sub(principal)
            .ok_or(LendingError::ArithmeticOverflow)?;
        
        self.available_liquidity = self.available_liquidity
            .checked_sub(total_amount)
            .ok_or(LendingError::ArithmeticOverflow)?;

        Ok(())
    }

    /// handle borrow - calculate interest based on current rate
    pub fn borrow(&mut self, amount: u64) -> Result<()> {
        require!(amount >= self.min_borrow_amount, LendingError::BelowMinimumDeposit);
        require!(!self.is_paused, LendingError::PoolPaused);

        require!(
            self.available_liquidity >= amount,
            LendingError::InsufficientLiquidity
        );

        self.available_liquidity = self.available_liquidity
            .checked_sub(amount)
            .ok_or(LendingError::ArithmeticOverflow)?;
        
        self.total_borrowed = self.total_borrowed
            .checked_add(amount)
            .ok_or(LendingError::ArithmeticOverflow)?;

        Ok(())
    }

    /// handle repay - calculate interest based on current rate
    pub fn repay(&mut self, principal: u64, interest: u64) -> Result<()> {
        let total_amount = principal
            .checked_add(interest)
            .ok_or(LendingError::ArithmeticOverflow)?;

        self.total_borrowed = self.total_borrowed
            .checked_sub(principal)
            .ok_or(LendingError::ArithmeticOverflow)?;
        
        // principal + interest increase available liquidity
        self.available_liquidity = self.available_liquidity
            .checked_add(total_amount)
            .ok_or(LendingError::ArithmeticOverflow)?;

        Ok(())
    }

    /// update rates - only admin can call
    pub fn update_rates(&mut self, new_deposit_rate: Option<u16>, new_borrow_rate: Option<u16>) -> Result<()> {
        if let Some(rate) = new_deposit_rate {
            require!(rate <= 2000, LendingError::InvalidRiskParameters); // max 20%
            self.current_deposit_rate = rate;
        }
        
        if let Some(rate) = new_borrow_rate {
            require!(rate <= 2000, LendingError::InvalidRiskParameters); // max 20%
            self.current_borrow_rate = rate;
        }
        
        self.last_rate_update = Clock::get().unwrap().unix_timestamp;
        Ok(())
    }

    /// calculate deposit interest - based on current rate
    /// formula: interest = principal × current_deposit_rate × minutes / (365 × 24 × 60)
    pub fn calculate_deposit_interest(
        &self,
        principal: u64,
        deposit_timestamp: i64,
    ) -> Result<u64> {
        let current_time = Clock::get().unwrap().unix_timestamp;
        let duration_seconds = current_time
            .checked_sub(deposit_timestamp)
            .ok_or(LendingError::InvalidPrice)? as u64;

        // convert to minutes (round down)
        let duration_minutes = duration_seconds / 60;

        // calculate interest: principal × current_deposit_rate × minutes / (365 days × 24 hours × 60 minutes × 10000)
        // 365 × 24 × 60 = 525,600 minutes/year
        let interest = (principal as u128)
            .checked_mul(self.current_deposit_rate as u128)
            .ok_or(LendingError::ArithmeticOverflow)?
            .checked_mul(duration_minutes as u128)
            .ok_or(LendingError::ArithmeticOverflow)?
            .checked_div(525_600 * constants::BASIS_POINTS)
            .ok_or(LendingError::ArithmeticOverflow)? as u64;

        Ok(interest)
    }

    /// calculate borrow interest - based on current rate
    /// formula: interest = principal × borrow_rate × minutes / (365 × 24 × 60)
    /// round down to ensure accurate repayment
    pub fn calculate_borrow_interest(
        principal: u64,
        borrow_rate: u16,
        borrow_timestamp: i64,
    ) -> Result<u64> {
        let current_time = Clock::get().unwrap().unix_timestamp;
        let duration_seconds = current_time
            .checked_sub(borrow_timestamp)
            .ok_or(LendingError::InvalidPrice)? as u64;

        // convert to minutes (round down)
        let duration_minutes = duration_seconds / 60;

        // calculate interest: principal × borrow_rate × minutes / (365 days × 24 hours × 60 minutes × 10000)
        // 365 × 24 × 60 = 525,600 minutes/year
        let interest = (principal as u128)
            .checked_mul(borrow_rate as u128)
            .ok_or(LendingError::ArithmeticOverflow)?
            .checked_mul(duration_minutes as u128)
            .ok_or(LendingError::ArithmeticOverflow)?
            .checked_div(525_600 * constants::BASIS_POINTS)
            .ok_or(LendingError::ArithmeticOverflow)? as u64;

        Ok(interest)
    }
}

/// Deposit record for each user deposit
#[account]
pub struct UserDepositRecord {
    /// Deposit user
    pub owner: Pubkey,
    /// Associated pool
    pub pool: Pubkey,
    /// Principal deposit amount
    pub principal_amount: u64,
    /// Deposit timestamp
    pub deposit_timestamp: i64,
    /// Deposit index (user's nth deposit)
    pub deposit_index: u32,
    /// Whether withdrawn
    pub is_withdrawn: bool,
    /// Withdraw timestamp (if withdrawn)
    pub withdraw_timestamp: i64,
    /// Earned interest (recorded at withdraw time)
    pub earned_interest: u64,
    /// PDA bump
    pub bump: u8,
}

impl UserDepositRecord {
    pub const LEN: usize = 8 + // discriminator
        32 + // owner
        32 + // pool
        8 +  // principal_amount
        8 +  // deposit_timestamp
        4 +  // deposit_index
        1 +  // is_withdrawn
        8 +  // withdraw_timestamp
        8 +  // earned_interest
        1;   // bump

    pub fn initialize(
        &mut self,
        owner: Pubkey,
        pool: Pubkey,
        principal_amount: u64,
        deposit_index: u32,
        bump: u8,
    ) {
        self.owner = owner;
        self.pool = pool;
        self.principal_amount = principal_amount;
        self.deposit_timestamp = Clock::get().unwrap().unix_timestamp;
        self.deposit_index = deposit_index;
        self.is_withdrawn = false;
        self.withdraw_timestamp = 0;
        self.earned_interest = 0;
        self.bump = bump;
    }

    pub fn mark_withdrawn(&mut self, earned_interest: u64) {
        self.is_withdrawn = true;
        self.withdraw_timestamp = Clock::get().unwrap().unix_timestamp;
        self.earned_interest = earned_interest;
    }
}

/// Borrow record for each user borrow
#[account]
pub struct UserBorrowRecord {
    /// Borrow user
    pub owner: Pubkey,
    /// Associated pool
    pub pool: Pubkey,
    /// Principal borrow amount
    pub principal_amount: u64,
    /// Locked borrow rate (at borrow time)
    pub locked_borrow_rate: u16,
    /// Borrow timestamp
    pub borrow_timestamp: i64,
    /// Borrow index (user's nth borrow)
    pub borrow_index: u32,
    /// Remaining principal to repay
    pub remaining_principal: u64,
    /// Whether repaid
    pub is_repaid: bool,
    /// Last repay timestamp
    pub last_repay_timestamp: i64,
    /// Total repaid principal
    pub total_repaid_principal: u64,
    /// Total repaid interest
    pub total_repaid_interest: u64,
    /// PDA bump
    pub bump: u8,
}

impl UserBorrowRecord {
    pub const LEN: usize = 8 + // discriminator
        32 + // owner
        32 + // pool
        8 +  // principal_amount
        2 +  // locked_borrow_rate
        8 +  // borrow_timestamp
        4 +  // borrow_index
        8 +  // remaining_principal
        1 +  // is_repaid
        8 +  // last_repay_timestamp
        8 +  // total_repaid_principal
        8 +  // total_repaid_interest
        1;   // bump

    pub fn initialize(
        &mut self,
        owner: Pubkey,
        pool: Pubkey,
        principal_amount: u64,
        locked_borrow_rate: u16,
        borrow_index: u32,
        bump: u8,
    ) {
        self.owner = owner;
        self.pool = pool;
        self.principal_amount = principal_amount;
        self.locked_borrow_rate = locked_borrow_rate;
        self.borrow_timestamp = Clock::get().unwrap().unix_timestamp;
        self.borrow_index = borrow_index;
        self.remaining_principal = principal_amount;
        self.is_repaid = false;
        self.last_repay_timestamp = 0;
        self.total_repaid_principal = 0;
        self.total_repaid_interest = 0;
        self.bump = bump;
    }

    pub fn process_repay(&mut self, principal_repaid: u64, interest_repaid: u64) {
        self.remaining_principal = self.remaining_principal.saturating_sub(principal_repaid);
        self.total_repaid_principal = self.total_repaid_principal.saturating_add(principal_repaid);
        self.total_repaid_interest = self.total_repaid_interest.saturating_add(interest_repaid);
        self.last_repay_timestamp = Clock::get().unwrap().unix_timestamp;
        
        if self.remaining_principal == 0 {
            self.is_repaid = true;
        }
    }

    /// Calculate current interest due (always from borrow time)
    pub fn calculate_current_interest(&self) -> Result<u64> {

        // Using borrow time to calculate interest
        TraditionalUsdcPool::calculate_borrow_interest(
            self.remaining_principal,
            self.locked_borrow_rate,
            self.borrow_timestamp, // Always calculate from borrow time
        )
    }
}

/// User repay record - detailed information of each repay
#[account]
pub struct UserRepayRecord {
    /// Repay user
    pub owner: Pubkey,
    /// Associated pool
    pub pool: Pubkey,
    /// Associated borrow record mint (for association)
    pub lp_mint: Pubkey,
    /// Principal amount repaid
    pub principal_repaid: u64,
    /// Interest amount repaid
    pub interest_repaid: u64,
    /// Total amount repaid (principal + interest)
    pub total_repaid: u64,
    /// Repay timestamp
    pub repay_timestamp: i64,
    /// Repay index (user's nth repay)
    pub repay_index: u32,
    /// Associated borrow record index
    pub related_borrow_index: u32,
    /// PDA bump
    pub bump: u8,
}

impl UserRepayRecord {
    pub const LEN: usize = 8 + // discriminator
        32 + // owner
        32 + // pool
        32 + // lp_mint
        8 +  // principal_repaid
        8 +  // interest_repaid
        8 +  // total_repaid
        8 +  // repay_timestamp
        4 +  // repay_index
        4 +  // related_borrow_index
        1;   // bump

    pub fn initialize(
        &mut self,
        owner: Pubkey,
        pool: Pubkey,
        lp_mint: Pubkey,
        principal_repaid: u64,
        interest_repaid: u64,
        repay_index: u32,
        related_borrow_index: u32,
        bump: u8,
    ) {
        self.owner = owner;
        self.pool = pool;
        self.lp_mint = lp_mint;
        self.principal_repaid = principal_repaid;
        self.interest_repaid = interest_repaid;
        self.total_repaid = principal_repaid + interest_repaid;
        self.repay_timestamp = Clock::get().unwrap().unix_timestamp;
        self.repay_index = repay_index;
        self.related_borrow_index = related_borrow_index;
        self.bump = bump;
    }
}

/// User withdraw record - detailed information of each withdraw
#[account]
pub struct UserWithdrawRecord {
    /// Withdraw user
    pub owner: Pubkey,
    /// Associated pool
    pub pool: Pubkey,
    /// Principal amount withdrawn
    pub principal_withdrawn: u64,
    /// Interest earned
    pub interest_earned: u64,
    /// Total withdrawn (principal + interest)
    pub total_withdrawn: u64,
    /// Withdraw timestamp
    pub withdraw_timestamp: i64,
    /// Withdraw index (user's nth withdraw)
    pub withdraw_index: u32,
    /// Associated deposit record index
    pub related_deposit_index: u32,
    /// PDA bump
    pub bump: u8,
}

impl UserWithdrawRecord {
    pub const LEN: usize = 8 + // discriminator
        32 + // owner
        32 + // pool
        8 +  // principal_withdrawn
        8 +  // interest_earned
        8 +  // total_withdrawn
        8 +  // withdraw_timestamp
        4 +  // withdraw_index
        4 +  // related_deposit_index
        1;   // bump

    pub fn initialize(
        &mut self,
        owner: Pubkey,
        pool: Pubkey,
        principal_withdrawn: u64,
        interest_earned: u64,
        withdraw_index: u32,
        related_deposit_index: u32,
        bump: u8,
    ) {
        self.owner = owner;
        self.pool = pool;
        self.principal_withdrawn = principal_withdrawn;
        self.interest_earned = interest_earned;
        self.total_withdrawn = principal_withdrawn + interest_earned;
        self.withdraw_timestamp = Clock::get().unwrap().unix_timestamp;
        self.withdraw_index = withdraw_index;
        self.related_deposit_index = related_deposit_index;
        self.bump = bump;
    }
}