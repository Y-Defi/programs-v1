/// User position state structure

use anchor_lang::prelude::*;
use super::constants;

/// User position state - records each user's collateral and borrowing situation
#[account]
pub struct UserPosition {
    /// User wallet address
    pub owner: Pubkey,
    /// User's LP token mint address
    pub lp_mint: Pubkey,
    /// Amount of LP tokens deposited
    pub lp_deposited: u64,
    /// Amount of USDC borrowed
    pub usdc_borrowed: u64,
    /// Collateral value cache (USD, 6 decimal places)
    pub collateral_value_cache: u64,
    /// Collateral value in USD (used for health factor calculation)
    pub collateral_value_usd: u64,
    /// Maximum borrowable amount (based on 50% collateral ratio)
    pub max_borrow_amount: u64,
    /// Health factor cache
    pub health_factor_cache: u64,
    /// Last cache update timestamp
    pub last_cache_update: i64,
    /// Position creation timestamp
    pub created_at: i64,
    /// Whether the position is liquidated
    pub is_liquidated: bool,
    /// Whether liquidation is pending
    pub liquidation_pending: bool,
    /// Remaining collateral value after liquidation (USDC)
    pub liquidated_collateral: u64,
    /// Health factor (6 decimal places)
    pub health_factor: u64,
    /// Last update slot number
    pub last_update_slot: u64,
    /// Remaining assets after liquidation
    pub remaining_assets: u64,
    /// Claimable USDC amount (new liquidation model)
    pub claimable_usdc: u64,
    /// Total USDC received from liquidation (for recordkeeping)
    pub liquidation_usdc_received: u64,
    /// Total borrow records (for PDA seed generation)
    pub total_borrow_count: u32,
    /// Total LP deposit records (for PDA seed generation)
    pub total_lp_deposit_count: u32,
    /// Total LP withdraw records (for PDA seed generation)
    pub total_lp_withdraw_count: u32,
    /// Total USDC repay records (for PDA seed generation)
    pub total_repay_count: u32,
    /// Total USDC withdraw records (for PDA seed generation)
    pub total_withdraw_count: u32,
    /// PDA bump
    pub bump: u8,
}

impl UserPosition {
    /// Account space size
    pub const LEN: usize = 8 + // discriminator
        32 + // owner
        32 + // lp_mint
        8 +  // lp_deposited
        8 +  // usdc_borrowed
        8 +  // collateral_value_cache
        8 +  // collateral_value_usd
        8 +  // max_borrow_amount
        8 +  // health_factor_cache
        8 +  // last_cache_update
        8 +  // created_at
        1 +  // is_liquidated
        1 +  // liquidation_pending
        8 +  // liquidated_collateral
        8 +  // health_factor
        8 +  // last_update_slot
        8 +  // remaining_assets
        8 +  // claimable_usdc
        8 +  // liquidation_usdc_received
        4 +  // total_borrow_count
        4 +  // total_lp_deposit_count
        4 +  // total_lp_withdraw_count
        4 +  // total_repay_count
        4 +  // total_withdraw_count
        1;   // bump

    /// Initialize user position
    pub fn initialize(&mut self, owner: Pubkey, lp_mint: Pubkey, bump: u8) {
        self.owner = owner;
        self.lp_mint = lp_mint;
        self.lp_deposited = 0;
        self.usdc_borrowed = 0;
        self.collateral_value_cache = 0;
        self.collateral_value_usd = 0;
        self.max_borrow_amount = 0;
        self.health_factor_cache = u64::MAX;
        self.last_cache_update = 0;
        self.created_at = Clock::get().unwrap().unix_timestamp;
        self.is_liquidated = false;
        self.liquidation_pending = false;
        self.liquidated_collateral = 0;
        self.health_factor = u64::MAX;
        self.last_update_slot = 0;
        self.remaining_assets = 0;
        self.claimable_usdc = 0;
        self.liquidation_usdc_received = 0;
        self.total_borrow_count = 0;
        self.total_lp_deposit_count = 0;
        self.total_lp_withdraw_count = 0;
        self.total_repay_count = 0;
        self.total_withdraw_count = 0;
        self.bump = bump;
    }

    /// Deposit LP tokens
    pub fn deposit_lp(&mut self, amount: u64) {
        self.lp_deposited = self.lp_deposited.saturating_add(amount);
        self.invalidate_cache();
    }

    /// Withdraw LP tokens
    pub fn withdraw_lp(&mut self, amount: u64) {
        // CLMM NFT, if withdraw amount is 1 and lp_deposited is greater than 1_000_000, reset to 0
        // Because lp_deposited may store liquidity amount instead of NFT amount
        if amount == 1 && self.lp_deposited > 1_000_000 {
            // CLMM NFT case, reset to 0
            self.lp_deposited = 0;
        } else {
            // Traditional LP token case
            self.lp_deposited = self.lp_deposited.saturating_sub(amount);
        }
        self.invalidate_cache();
    }

    /// Add borrow amount
    pub fn add_borrow(&mut self, amount: u64) {
        self.usdc_borrowed = self.usdc_borrowed.saturating_add(amount);
        self.invalidate_cache();
    }

    /// Repay borrow amount
    pub fn repay_borrow(&mut self, amount: u64) {
        self.usdc_borrowed = self.usdc_borrowed.saturating_sub(amount);
        self.invalidate_cache();
    }

    /// Update collateral value cache and health factor cache
    pub fn update_cache(&mut self, collateral_value: u64, health_factor: u64) {
        self.collateral_value_cache = collateral_value;
        self.health_factor_cache = health_factor;
        self.last_cache_update = Clock::get().unwrap().unix_timestamp;
    }

    /// Invalidate cache
    fn invalidate_cache(&mut self) {
        self.last_cache_update = 0;
    }

    /// Check if cache is stale (5 minutes)
    pub fn is_cache_stale(&self) -> bool {
        let current_time = Clock::get().unwrap().unix_timestamp;
        current_time - self.last_cache_update > 300
    }

    /// Check if user has debt
    pub fn has_debt(&self) -> bool {
        self.usdc_borrowed > 0
    }
    
    /// Calculate available borrow capacity
    pub fn available_borrow_capacity(&self, collateral_ratio: u16) -> u64 {
        let max_borrow = (self.collateral_value_usd as u128)
            .saturating_mul(collateral_ratio as u128)
            .saturating_div(constants::BASIS_POINTS) as u64;
        
        if max_borrow > self.usdc_borrowed {
            max_borrow.saturating_sub(self.usdc_borrowed)
        } else {
            0
        }
    }

    /// Mark position as liquidated
    pub fn mark_liquidated(&mut self, remaining_collateral: u64) {
        self.is_liquidated = true;
        self.liquidated_collateral = remaining_collateral;
        self.usdc_borrowed = 0; // Debt is fully repaid
        self.lp_deposited = 0;  // LP is fully liquidated
        self.invalidate_cache();
    }

    /// Claim liquidated assets
    pub fn claim_liquidated_assets(&mut self) -> u64 {
        let claimable = self.liquidated_collateral;
        self.liquidated_collateral = 0;
        claimable
    }
    
    /// Set claimable USDC amount (new liquidation model)
    pub fn set_claimable_usdc(&mut self, amount: u64) {
        self.claimable_usdc = amount;
    }
    
    /// Claim USDC (new liquidation model)
    pub fn claim_usdc(&mut self) -> u64 {
        let claimable = self.claimable_usdc;
        self.claimable_usdc = 0;
        claimable
    }
    
    /// Record liquidation USDC amount (new liquidation model)
    pub fn record_liquidation_usdc(&mut self, amount: u64) {
        self.liquidation_usdc_received = self.liquidation_usdc_received.saturating_add(amount);
    }
}

/// LP Deposit Record - Record each LP deposit transaction
#[account]
pub struct LpDepositRecord {
    /// Deposit user
    pub owner: Pubkey,
    /// LP mint address
    pub lp_mint: Pubkey,
    /// LP amount deposited
    pub lp_amount: u64,
    /// LP value at deposit (USD, 8 decimal places)
    pub lp_value_at_deposit: u64,
    /// Deposit timestamp
    pub deposit_timestamp: i64,
    /// Deposit index (user's nth LP deposit)
    pub deposit_index: u32,
    /// Whether withdrawn
    pub is_withdrawn: bool,
    /// Withdraw timestamp (if withdrawn)
    pub withdraw_timestamp: i64,
    /// PDA bump
    pub bump: u8,
}

impl LpDepositRecord {
    pub const LEN: usize = 8 + // discriminator
        32 + // owner
        32 + // lp_mint
        8 +  // lp_amount
        8 +  // lp_value_at_deposit
        8 +  // deposit_timestamp
        4 +  // deposit_index
        1 +  // is_withdrawn
        8 +  // withdraw_timestamp
        1;   // bump

    pub fn initialize(
        &mut self,
        owner: Pubkey,
        lp_mint: Pubkey,
        lp_amount: u64,
        lp_value: u64,
        deposit_index: u32,
        bump: u8,
    ) {
        self.owner = owner;
        self.lp_mint = lp_mint;
        self.lp_amount = lp_amount;
        self.lp_value_at_deposit = lp_value;
        self.deposit_timestamp = Clock::get().unwrap().unix_timestamp;
        self.deposit_index = deposit_index;
        self.is_withdrawn = false;
        self.withdraw_timestamp = 0;
        self.bump = bump;
    }

    pub fn mark_withdrawn(&mut self) {
        self.is_withdrawn = true;
        self.withdraw_timestamp = Clock::get().unwrap().unix_timestamp;
    }
}

/// LP Withdraw Record - Record each LP withdraw transaction
#[account]
pub struct LpWithdrawRecord {
    /// Withdraw user
    pub owner: Pubkey,
    /// LP mint address
    pub lp_mint: Pubkey,
    /// LP amount withdrawn
    pub lp_amount: u64,
    /// LP value at withdraw (USD, 8 decimal places)
    pub lp_value_at_withdraw: u64,
    /// Withdraw timestamp
    pub withdraw_timestamp: i64,
    /// Withdraw index (user's nth LP withdraw)
    pub withdraw_index: u32,
    /// Related deposit index (if withdraw is for a specific deposit)
    pub related_deposit_index: u32,
    /// PDA bump
    pub bump: u8,
}

impl LpWithdrawRecord {
    pub const LEN: usize = 8 + // discriminator
        32 + // owner
        32 + // lp_mint
        8 +  // lp_amount
        8 +  // lp_value_at_withdraw
        8 +  // withdraw_timestamp
        4 +  // withdraw_index
        4 +  // related_deposit_index
        1;   // bump

    pub fn initialize(
        &mut self,
        owner: Pubkey,
        lp_mint: Pubkey,
        lp_amount: u64,
        lp_value: u64,
        withdraw_index: u32,
        bump: u8,
    ) {
        self.owner = owner;
        self.lp_mint = lp_mint;
        self.lp_amount = lp_amount;
        self.lp_value_at_withdraw = lp_value;
        self.withdraw_timestamp = Clock::get().unwrap().unix_timestamp;
        self.withdraw_index = withdraw_index;
        self.related_deposit_index = u32::MAX; 
        self.bump = bump;
    }
}