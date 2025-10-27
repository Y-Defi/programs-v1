// Pool State - Manage global pool information and Raydium pool state

use anchor_lang::prelude::*;

// Global Pool State - Manage global pool information and Raydium pool state
#[account]
pub struct GlobalPool {
    /// Pool authority
    pub authority: Pubkey,
    /// Total locked LP tokens
    pub total_lp_locked: u64,
    /// Total borrowed USDC amount
    pub total_usdc_borrowed: u64,
    /// USDC reserve pool amount
    pub usdc_reserve: u64,
    /// LP token mint address   
    pub lp_token_mint: Pubkey,
    /// USDC token mint address
    pub usdc_mint: Pubkey,
    /// Raydium pool address
    pub raydium_pool_id: Pubkey,
    /// Collateral ratio (50% = 5000 / 10000)
    pub collateral_ratio: u16,
    /// Discount rate (80% = 8000 / 10000)
    pub discount_rate: u16,
    /// Liquidation threshold (110% = 11000 / 10000)
    pub liquidation_threshold: u16,
    /// Pool creation timestamp
    pub created_at: i64,
    /// Last update timestamp
    pub updated_at: i64,
}

impl GlobalPool {
    /// Account space size
    pub const LEN: usize = 8 + // discriminator
        32 + // authority
        8 +  // total_lp_locked
        8 +  // total_usdc_borrowed
        8 +  // usdc_reserve
        32 + // lp_token_mint
        32 + // usdc_mint
        32 + // raydium_pool_id
        2 +  // collateral_ratio
        2 +  // discount_rate
        2 +  // liquidation_threshold
        8 +  // created_at
        8;   // updated_at
}

// Raydium Pool Info Cache - Used to optimize price calculations
#[account]
pub struct RaydiumPoolInfo {
    /// Pool ID
    pub pool_id: Pubkey,
    /// SOL reserve amount
    pub coin_reserve: u64,
    /// USDC reserve amount
    pub pc_reserve: u64,
    /// Total LP supply
    pub lp_supply: u64,
    /// Last update timestamp
    pub last_update: i64,
    /// Data validity duration (seconds)
    pub validity_duration: i64,
}

impl RaydiumPoolInfo {
    /// Account space size
    pub const LEN: usize = 8 + // discriminator
        32 + // pool_id
        8 +  // coin_reserve
        8 +  // pc_reserve
        8 +  // lp_supply
        8 +  // last_update
        8;   // validity_duration

    /// Check if data is stale
    pub fn is_stale(&self) -> bool {
        let current_time = Clock::get().unwrap().unix_timestamp;
        current_time - self.last_update > self.validity_duration
    }

    /// Update pool information
    pub fn update(&mut self, coin_reserve: u64, pc_reserve: u64, lp_supply: u64) {
        self.coin_reserve = coin_reserve;
        self.pc_reserve = pc_reserve;
        self.lp_supply = lp_supply;
        self.last_update = Clock::get().unwrap().unix_timestamp;
    }
}