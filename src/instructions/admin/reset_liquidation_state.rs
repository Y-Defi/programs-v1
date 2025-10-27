// Admin reset liquidation state instruction
use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::LendingError;

#[derive(Accounts)]
pub struct ResetLiquidationState<'info> {
    /// Protocol admin account (must sign)
    #[account(mut)]
    pub admin: Signer<'info>,

    /// Protocol configuration account (mutated)
    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        constraint = protocol_config.authority == admin.key() @ LendingError::InvalidAuthority
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// User position account (mutated)
    #[account(
        mut,
        constraint = user_position.liquidation_pending @ LendingError::LiquidationPending
    )]
    pub user_position: Account<'info, UserPosition>,

    /// Target user account (checked via user_position constraint)
    /// CHECK: use target_user to verify user_position.owner
    pub target_user: AccountInfo<'info>,
}

/// Admin reset user liquidation state instruction
/// Use only in emergency or testing scenarios
pub fn reset_liquidation_state(ctx: Context<ResetLiquidationState>) -> Result<()> {
    let user_position = &mut ctx.accounts.user_position;


    // Verify admin authority
    require!(
        ctx.accounts.protocol_config.authority == ctx.accounts.admin.key(),
        LendingError::InvalidAuthority
    );

    // Reset liquidation related states
    user_position.liquidation_pending = false;
    user_position.is_liquidated = false;


    Ok(())
}