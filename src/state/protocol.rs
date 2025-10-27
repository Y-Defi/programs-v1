/// Protocol configuration state structure
use anchor_lang::prelude::*;

/// Global protocol configuration - stores base parameters, pool and LP mint addresses are dynamically passed in, USDC pool is managed uniformly by traditional_usdc_pool
#[account]
pub struct ProtocolConfig {
    /// Protocol administrator
    pub authority: Pubkey,
    /// Collateral ratio (e.g., 5000 for 50%)
    pub collateral_ratio: u16,
    /// Liquidation threshold (e.g., 11000 for 110%)
    pub liquidation_threshold: u16,
    /// Default SOL price feed address (can be overridden in operations)
    pub sol_price_feed: Pubkey,
    /// USDC mint address
    pub usdc_mint: Pubkey,
    /// Total borrowed amount (from traditional pool)
    pub total_borrowed: u64,
    /// Total deposited amount
    pub total_deposited: u64,
    /// Total collateral value
    pub total_collateral_value: u64,
    /// Total number of users
    pub total_users: u32,
    /// Admin fee rate (1000 = 10%)
    pub admin_fee_rate: u16,
    /// Liquidator reward rate (300 = 3%)  
    pub liquidator_reward_rate: u16,
    /// Slippage tolerance (300 = 3%)
    pub slippage_tolerance: u16,
    /// Admin fee collection account PDA
    pub admin_fee_vault: Pubkey,
    /// Liquidation executor wallet address
    pub liquidation_executor: Pubkey,
    /// Total admin fees accumulated
    pub total_admin_fees: u64,
    /// Total number of liquidations
    pub total_liquidations: u64,
    /// Whether the protocol is paused
    pub is_paused: bool,
    /// Protocol creation timestamp
    pub created_at: i64,
    /// PDA bump
    pub bump: u8,
}

impl ProtocolConfig {
    /// Account space size
    pub const LEN: usize = 8 + // discriminator
        32 + // authority
        2 +  // collateral_ratio
        2 +  // liquidation_threshold
        32 + // sol_price_feed
        32 + // usdc_mint
        8 +  // total_borrowed
        8 +  // total_deposited
        8 +  // total_collateral_value
        4 +  // total_users
        2 +  // admin_fee_rate
        2 +  // liquidator_reward_rate
        2 +  // slippage_tolerance
        32 + // admin_fee_vault
        32 + // liquidation_executor
        8 +  // total_admin_fees
        8 +  // total_liquidations
        1 +  // is_paused
        8 +  // created_at
        1;   // bump
}