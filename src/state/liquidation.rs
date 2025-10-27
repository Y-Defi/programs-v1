// Liquidation related state structures
use anchor_lang::prelude::*;
use crate::instructions::liquidation::liquidation_context::LiquidationStatus;

/// Liquidation record - Stores detailed information about a liquidation event, including all required information for the TS script
#[account]
pub struct LiquidationRecord {
    /// Liquidated user
    pub liquidated_user: Pubkey,
    /// Liquidator
    pub liquidator: Pubkey,
    /// Liquidation status
    pub status: LiquidationStatus,


    /// LP token mint address
    pub lp_mint: Pubkey,
    /// Amount of LP tokens liquidated
    pub lp_amount: u64,
    /// Raydium pool ID
    pub pool_id: Pubkey,
    /// CLMM pool state address
    pub pool_state: Pubkey,
    /// Personal position address
    pub personal_position: Pubkey,

    // ===== Debt and value information =====
    /// Debt amount at liquidation trigger
    pub debt_amount: u64,
    /// LP token value in USD at liquidation trigger
    pub lp_value_at_trigger: u64,
    /// Health factor at liquidation trigger
    pub health_factor_at_trigger: u64,

    // ===== Transfer target addresses =====
    /// USDC pool vault address (for debt repayment)
    pub usdc_pool_vault: Pubkey,
    /// Admin fee vault address
    pub admin_fee_vault: Pubkey,
    /// Liquidator USDC account address
    pub liquidator_usdc_account: Pubkey,
    /// User USDC account address
    pub user_usdc_account: Pubkey,

    // ===== Execution result records =====
    /// Debt amount repaid
    pub debt_repaid: u64,
    /// User claimable amount
    pub user_claimable: u64,

    // ===== Time and status =====
    /// Trigger timestamp
    pub triggered_at: i64,
    /// Completion timestamp
    pub completed_at: i64,
    /// Whether user has claimed
    pub user_claimed: bool,
    /// PDA bump
    pub bump: u8,
}

impl LiquidationRecord {
    /// Account space size
    pub const LEN: usize = 8 + // discriminator
        32 + // liquidated_user
        32 + // liquidator
        1 +  // status (LiquidationStatus enum)
        // LP and pool information
        32 + // lp_mint
        8 +  // lp_amount
        32 + // pool_id
        32 + // pool_state
        32 + // personal_position
        // Debt and value information
        8 +  // debt_amount
        8 +  // lp_value_at_trigger
        8 +  // health_factor_at_trigger
        // Transfer target addresses
        32 + // usdc_pool_vault
        32 + // admin_fee_vault
        32 + // liquidator_usdc_account
        32 + // user_usdc_account
        // Execution result records
        8 +  // debt_repaid
        8 +  // user_claimable
        // Time and status
        8 +  // triggered_at
        8 +  // completed_at
        1 +  // user_claimed
        1;   // bump
}