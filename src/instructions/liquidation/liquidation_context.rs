// Simplified liquidation account struct - only handles state management and USDC distribution

use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};
use anchor_spl::token_interface::{TokenInterface};
use crate::state::*;
use crate::errors::LendingError;

/// Trigger liquidation - validate conditions and create liquidation record (optimized stack usage)
#[derive(Accounts)]
#[instruction(target_user: Pubkey)]
pub struct TriggerLiquidation<'info> {
    /// Liquidator account (must sign)
    #[account(mut)]
    pub liquidator: Signer<'info>,

    /// Protocol configuration account (mutated)
    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump,
        constraint = !protocol_config.is_paused @ LendingError::ProtocolPaused
    )]
    pub protocol_config: Box<Account<'info, ProtocolConfig>>,

    /// Target user's position account (mutated)
    #[account(
        mut,
        constraint = user_position.owner == target_user.key() @ LendingError::InvalidAuthority,
        constraint = user_position.usdc_borrowed > 0 @ LendingError::NoDebt,
        constraint = user_position.lp_deposited > 0 @ LendingError::NoCollateral,
        constraint = !user_position.liquidation_pending @ LendingError::LiquidationPending
    )]
    pub user_position: Box<Account<'info, UserPosition>>,

    /// Liquidation record account (initiated)
    #[account(
        init,
        payer = liquidator,
        space = LiquidationRecord::LEN,
        seeds = [
            LIQUIDATION_SEED,
            user_position.owner.as_ref(),
            liquidator.key().as_ref(),
            user_position.lp_mint.as_ref(),
        ],
        bump
    )]
    pub liquidation_record: Box<Account<'info, LiquidationRecord>>,

    /// Chainlink program account (for price validation)
    pub chainlink_program: AccountInfo<'info>,

    /// Chainlink SOL price feed account (for price validation)
    pub chainlink_sol_feed: AccountInfo<'info>,

    /// CLMM pool state account (for LP value calculation)
    /// CHECK: Use Raydium program to verify pool state
    pub clmm_pool_state: AccountInfo<'info>,

    /// User's PersonalPosition account (derived from NFT mint)
    /// CHECK: Verify correctness via PDA derivation
    pub personal_position: AccountInfo<'info>,

    /// Traditional USDC pool account (for USDC distribution)
    pub traditional_usdc_pool: Box<Account<'info, TraditionalUsdcPool>>,

    /// LP mint account (Token-2022 NFT mint)
    /// CHECK: Verify via user_position.lp_mint
    pub lp_mint: AccountInfo<'info>,

    /// User LP token account (protocol-owned)  
    /// CHECK: Manually verify owner and other constraints
    #[account(mut)]
    pub user_lp_vault: AccountInfo<'info>,

    /// Liquidation executor LP token vault account (protocol-owned)
    /// CHECK: Manually verify owner and other constraints
    #[account(mut)]
    pub liquidation_executor_lp_vault: AccountInfo<'info>,

    /// LP vault authority PDA
    /// CHECK: Verify via seeds derivation, used for LP token transfer permissions
    #[account(
        seeds = [b"lp_vault"],
        bump
    )]
    pub lp_vault_authority: AccountInfo<'info>,

    /// System program account
    pub system_program: Program<'info, System>,
    /// Token program account (supports Token-2022 and traditional Token)
    pub token_program: Interface<'info, TokenInterface>,
    /// Rent sysvar account
    pub rent: Sysvar<'info, Rent>,
}

/// Complete liquidation - update user status (TS script has completed distribution)
#[derive(Accounts)]
pub struct CompleteLiquidation<'info> {
    /// Admin or liquidation executor account (must sign)
    #[account(mut)]
    pub authority: Signer<'info>,

    /// Liquidation record account (mutated)
    #[account(
        mut,
        constraint = liquidation_record.status == LiquidationStatus::Triggered @ LendingError::InvalidLiquidationStatus
    )]
    pub liquidation_record: Account<'info, LiquidationRecord>,

    /// Protocol configuration account (mutated)
    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// Target user's position account (mutated)
    #[account(mut)]
    pub user_position: Account<'info, UserPosition>,
}

/// User claims remaining assets - records user claimed status
#[derive(Accounts)]
pub struct ClaimAssets<'info> {
    /// User account (must sign)
    #[account(mut)]
    pub user: Signer<'info>,

    /// User position account (mutated)
    #[account(
        mut,
        constraint = user_position.owner == user.key() @ LendingError::InvalidAuthority,
        constraint = user_position.claimable_usdc > 0 @ LendingError::NoClaimableAssets
    )]
    pub user_position: Account<'info, UserPosition>,

    /// Liquidation record account (mutated)
    #[account(
        mut,
        constraint = liquidation_record.liquidated_user == user.key() @ LendingError::InvalidAuthority,
        constraint = liquidation_record.status == LiquidationStatus::Completed @ LendingError::InvalidLiquidationStatus
    )]
    pub liquidation_record: Account<'info, LiquidationRecord>,
}

/// Liquidation status enum
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Debug)]
pub enum LiquidationStatus {
    Triggered,  // Liquidation triggered, waiting for TS script completion
    Completed,  // Liquidation completed
}

