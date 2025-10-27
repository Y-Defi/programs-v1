// Query Module - Module Entry and Public Type Definitions
use anchor_lang::prelude::*;

// Submodules
pub mod user_queries;
pub mod admin_queries;
pub mod pool_queries;
pub mod interest_queries;
pub mod batch_queries;

// Re-export all public interfaces
pub use user_queries::*;
pub use admin_queries::*;
pub use pool_queries::*;
pub use interest_queries::*;
pub use batch_queries::*;

// ============ Public Data Structures ============

/// User Position Information Return Structure
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct UserPositionInfo {
    pub owner: Pubkey,
    pub lp_mint: Pubkey,
    pub lp_deposited: u64,
    pub usdc_borrowed: u64,
    pub collateral_value_cache: u64,
    pub health_factor_cache: u64,
    pub cache_timestamp: i64,
    pub total_borrow_count: u32,
    pub is_liquidated: bool,
}

/// Borrow Record Information
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct BorrowRecordInfo {
    pub record_id: u32,
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub principal_amount: u64,
    pub locked_borrow_rate: u16,
    pub borrow_timestamp: i64,
    pub is_repaid: bool,
    pub total_repaid: u64,
    pub last_repay_timestamp: i64,
}

/// Deposit Record Information
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct DepositRecordInfo {
    pub deposit_id: u32,
    pub owner: Pubkey,
    pub pool: Pubkey,
    pub principal_amount: u64,
    pub deposit_timestamp: i64,
    pub is_withdrawn: bool,
    pub earned_interest: u64,
    pub withdraw_timestamp: i64,
}

/// LP Value Breakdown
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct LpValueBreakdown {
    pub token0_amount: u64,  // SOL amount in lamports
    pub token1_amount: u64,  // USDC amount in micro units
    pub token0_value_usd: u64,
    pub token1_value_usd: u64,
    pub total_value_usd: u64,
    pub sol_price_usd: u64,
}