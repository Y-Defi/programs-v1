// Core logic for liquidation process
// TS script responsible for Raydium operations

use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Transfer, TokenAccount};
use anchor_spl::token_interface::{TokenAccount as TokenInterfaceAccount, transfer_checked, TransferChecked};
use crate::state::*;
use crate::errors::LendingError;
use super::liquidation_context::*;
use super::liquidation_validation::validate_liquidation_conditions;

/// Trigger liquidation process - validate conditions, transfer LP to admin, create liquidation record
pub fn trigger_liquidation<'info>(
    ctx: Context<'_, '_, 'info, 'info, TriggerLiquidation<'info>>,
    target_user: Pubkey,
) -> Result<()> {

    // Manual validation of TokenAccount accounts (due to stack optimization)
    validate_token_accounts(&ctx)?;

    // Validate liquidation conditions and health factor
    let (health_factor, lp_value) = validate_liquidation_conditions(&ctx, target_user)?;

    let user_position = &mut ctx.accounts.user_position;
    let lp_amount = user_position.lp_deposited;

    // Set liquidation pending status to prevent user from repaying debt or taking LP during liquidation
    user_position.liquidation_pending = true;

    // Transfer LP tokens to liquidation executor vault for liquidation - using Token Interface for Token-2022 support
    transfer_checked(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            TransferChecked {
                from: ctx.accounts.user_lp_vault.to_account_info(),
                mint: ctx.accounts.lp_mint.to_account_info(),
                to: ctx.accounts.liquidation_executor_lp_vault.to_account_info(),
                authority: ctx.accounts.lp_vault_authority.to_account_info(),
            },
            &[&[b"lp_vault", &[ctx.bumps.lp_vault_authority]]]
        ),
        lp_amount,
        0, 
    )?;


    // Calculate total debt before mutable borrow to avoid borrowing conflicts
    let user_principal_debt = user_position.usdc_borrowed;
    let lp_mint = user_position.lp_mint;
    let total_debt = calculate_user_total_debt_from_remaining_accounts(&ctx, target_user, lp_mint)?;
    let estimated_interest = total_debt.saturating_sub(user_principal_debt);

    // Parse pool_id from personal position account data (offset: 41-73 bytes)
    let position_data = ctx.accounts.personal_position.data.borrow();
    let pool_id = Pubkey::try_from(&position_data[41..73])
        .map_err(|_| LendingError::InvalidPositionNft)?;
    drop(position_data); // Immediately release borrow after use

    // Create liquidation record with all required information for TS script
    let liquidation_record = &mut ctx.accounts.liquidation_record;
    let current_timestamp = Clock::get()?.unix_timestamp;

    // Basic information
    liquidation_record.liquidated_user = target_user;
    liquidation_record.liquidator = ctx.accounts.liquidator.key();
    liquidation_record.status = LiquidationStatus::Triggered;

    // LP and pool information (from accounts and user position)
    liquidation_record.lp_mint = lp_mint;
    liquidation_record.lp_amount = lp_amount;
    liquidation_record.pool_id = pool_id;

    liquidation_record.pool_state = ctx.accounts.clmm_pool_state.key();
    liquidation_record.personal_position = ctx.accounts.personal_position.key();

    liquidation_record.debt_amount = total_debt;
    liquidation_record.lp_value_at_trigger = lp_value;
    liquidation_record.health_factor_at_trigger = health_factor;


    // Transfer targets (from protocol config and pool account)
    liquidation_record.usdc_pool_vault = ctx.accounts.traditional_usdc_pool.pool_usdc_vault;
    liquidation_record.admin_fee_vault = ctx.accounts.protocol_config.admin_fee_vault;
    liquidation_record.liquidator_usdc_account = Pubkey::default(); // TS script will calculate
    liquidation_record.user_usdc_account = Pubkey::default(); // TS script will calculate

    // Execution results (initialized to 0)
    liquidation_record.debt_repaid = 0;
    liquidation_record.user_claimable = 0;

    // Timestamps and status
    liquidation_record.triggered_at = current_timestamp;
    liquidation_record.completed_at = 0;
    liquidation_record.user_claimed = false;
    liquidation_record.bump = ctx.bumps.liquidation_record;


    Ok(())
}

/// Complete liquidation - update debt and LP status (TS script has already distributed assets)
pub fn complete_liquidation<'info>(
    ctx: Context<'_, '_, '_, 'info, CompleteLiquidation<'info>>,
    debt_repaid: u64,
    user_claimable: u64,
) -> Result<()> {


    let liquidation_record = &mut ctx.accounts.liquidation_record;
    let user_position = &mut ctx.accounts.user_position;

    // Validate liquidation record status
    require!(
        liquidation_record.status == LiquidationStatus::Triggered,
        LendingError::InvalidLiquidationStatus
    );

    // Validate repay amount
    require!(
        debt_repaid <= liquidation_record.debt_amount,
        LendingError::InvalidRepayAmount
    );

    // Update user debt status
    user_position.usdc_borrowed = user_position.usdc_borrowed.saturating_sub(debt_repaid);

    // Liquidate LP collateral (already transferred to admin)
    user_position.lp_deposited = 0;

    // Record user claimable amount
    user_position.claimable_usdc = user_claimable;

    // Remove liquidation pending status
    user_position.liquidation_pending = false;
    user_position.is_liquidated = user_position.usdc_borrowed == 0;

    // Update protocol statistics
    let protocol_config = &mut ctx.accounts.protocol_config;
    protocol_config.total_borrowed = protocol_config.total_borrowed.saturating_sub(debt_repaid);
    protocol_config.total_liquidations = protocol_config.total_liquidations.saturating_add(1);

    // Update liquidation record
    liquidation_record.debt_repaid = debt_repaid;
    liquidation_record.user_claimable = user_claimable;
    liquidation_record.status = LiquidationStatus::Completed;
    liquidation_record.completed_at = Clock::get()?.unix_timestamp;


    Ok(())
}

/// User claims remaining assets after liquidation - update status (admin wallet already transferred)
pub fn claim_remaining_assets<'info>(
    ctx: Context<'_, '_, '_, 'info, ClaimAssets<'info>>,
) -> Result<()> {

    let user_position = &mut ctx.accounts.user_position;
    let liquidation_record = &mut ctx.accounts.liquidation_record;
    let claimable_amount = user_position.claimable_usdc;

    require!(claimable_amount > 0, LendingError::NoClaimableAssets);

    // Record user claimed status
    user_position.claimable_usdc = 0;
    liquidation_record.user_claimed = true;


    Ok(())
}

/// Manually validate TokenAccount (optimized stack version)
fn validate_token_accounts<'info>(
    ctx: &Context<'_, '_, '_, 'info, TriggerLiquidation<'info>>
) -> Result<()> {
    // Validate user LP vault account - supports Token-2022 and traditional Token   
    let user_lp_vault_data = ctx.accounts.user_lp_vault.try_borrow_data()?;
    let user_lp_vault_owner = if let Ok(account) = TokenInterfaceAccount::try_deserialize(&mut user_lp_vault_data.as_ref()) {
        account.owner
    } else {
        TokenAccount::try_deserialize(&mut user_lp_vault_data.as_ref())?.owner
    };

    require_eq!(
        user_lp_vault_owner,
        ctx.accounts.lp_vault_authority.key(),
        LendingError::InvalidTokenAccountOwner
    );

    // Validate liquidation executor LP vault account - supports Token-2022 and traditional Token
    let liquidation_executor_lp_vault_data = ctx.accounts.liquidation_executor_lp_vault.try_borrow_data()?;
    let liquidation_executor_lp_vault_owner = if let Ok(account) = TokenInterfaceAccount::try_deserialize(&mut liquidation_executor_lp_vault_data.as_ref()) {
        account.owner
    } else {
        // Traditional Token account
        TokenAccount::try_deserialize(&mut liquidation_executor_lp_vault_data.as_ref())?.owner
    };

    require_eq!(
        liquidation_executor_lp_vault_owner,
        ctx.accounts.protocol_config.liquidation_executor,
        LendingError::InvalidLiquidationExecutor
    );

    Ok(())
}

/// Calculate total debt (principal + interest) for a user from remaining accounts
fn calculate_user_total_debt_from_remaining_accounts<'info>(
    ctx: &Context<'_, '_, 'info, 'info, TriggerLiquidation<'info>>,
    target_user: Pubkey,
    lp_mint: Pubkey,
) -> Result<u64> {

    let mut total_debt = 0u64;
    let mut valid_records_count = 0u32;


    // Iterate through all remaining accounts to find valid borrow records
    for (index, account) in ctx.remaining_accounts.iter().enumerate() {
        if let Ok(borrow_record) = Account::<UserBorrowRecord>::try_from(account) {
            // Validate borrow record belongs to target user and correct LP mint
            if borrow_record.owner == target_user
                && borrow_record.pool == lp_mint
                && !borrow_record.is_repaid
                && borrow_record.remaining_principal > 0 {

                // Calculate interest for this borrow record
                let interest = TraditionalUsdcPool::calculate_borrow_interest(
                    borrow_record.remaining_principal,
                    borrow_record.locked_borrow_rate,
                    borrow_record.borrow_timestamp,
                )?;

                let record_total = borrow_record.remaining_principal
                    .checked_add(interest)
                    .ok_or(LendingError::ArithmeticOverflow)?;

                total_debt = total_debt
                    .checked_add(record_total)
                    .ok_or(LendingError::ArithmeticOverflow)?;

                valid_records_count += 1;
            }
        }
    }

    require!(total_debt > 0, LendingError::NoDebt);
    require!(valid_records_count > 0, LendingError::UserPositionNotFound);

    Ok(total_debt)
}

