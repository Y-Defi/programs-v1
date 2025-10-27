// Admin close liquidation record instruction
use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::LendingError;

#[derive(Accounts)]
pub struct CloseLiquidationRecord<'info> {
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

    /// Liquidation record account to close (will be closed)
    #[account(
        mut,
        close = admin // Return rent to admin
    )]
    pub liquidation_record: Account<'info, LiquidationRecord>,
}

/// Admin close liquidation record instruction
/// Use only in testing or cleanup scenarios
pub fn close_liquidation_record(ctx: Context<CloseLiquidationRecord>) -> Result<()> {
    let liquidation_record = &ctx.accounts.liquidation_record;


    // Verify admin authority
    require!(
        ctx.accounts.protocol_config.authority == ctx.accounts.admin.key(),
        LendingError::InvalidAuthority
    );


    Ok(())
}