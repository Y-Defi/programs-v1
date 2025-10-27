// Liquidation Condition Validation
use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::LendingError;
use crate::utils::{get_sol_price, apply_discount};
use crate::instructions::valuation::{
    calculate_clmm_position_value, 
    parse_personal_position_data, 
    parse_clmm_current_tick,
    calculate_token_amounts_from_liquidity,
    calculate_value_from_token_amounts,
    CalculateClmmPositionValue,
    PersonalPositionInfo
};
use super::liquidation_context::{TriggerLiquidation, LiquidationStatus};
use crate::state::DISCOUNT_FACTOR;

/// Stage 1: Complete Basic Validation + Real-time LP Value Calculation
pub fn validate_liquidation_conditions(
    ctx: &Context<TriggerLiquidation>, 
    target_user: Pubkey,
) -> Result<(u64, u64)> { // Return (health_factor, lp_current_value)
    
    let user_position = &ctx.accounts.user_position;
    let protocol_config = &ctx.accounts.protocol_config;
    
    
    // ============ Basic Validation ============
    
    // 1. Verify user authority
    require!(
        user_position.owner == target_user,
        LendingError::InvalidAuthority
    );
    
    // 2. Verify user has >= 1 LP and outstanding USDC debt
    require!(
        user_position.lp_deposited >= 1,
        LendingError::InsufficientCollateral
    );
    
    require!(
        user_position.usdc_borrowed > 0,
        LendingError::NoDebt
    );
    
    
    // 3. Price Data Validity and Timeliness Verification
    let sol_price = get_sol_price(
        &ctx.accounts.chainlink_program, 
        &ctx.accounts.chainlink_sol_feed
    )?;
    
    require!(sol_price > 0, LendingError::InvalidPrice);
    
    // 3.1 Check price timeliness (optional: add timestamp check)
    let current_timestamp = Clock::get()?.unix_timestamp;
    let max_price_age = 300; // 5分钟
    
    let last_update = user_position.last_cache_update;
    if last_update > 0 {
        let price_age = current_timestamp.saturating_sub(last_update);
        if price_age > max_price_age {
        }
    }
    
    
    // 4. Calculate current LP value using valuation.rs
    let (current_lp_value, nft_mint, position_info) = calculate_real_time_lp_value(
        ctx,
        &user_position,
        sol_price,
    )?;
    
    
    require!(current_lp_value > 0, LendingError::ZeroAmount);
    
    // 5. Apply discount factor to calculate effective collateral
    let effective_collateral = apply_discount(current_lp_value, DISCOUNT_FACTOR)?;
    
    // 6. Calculate current health factor - using standard function in utils, with 6 decimal precision
    let effective_collateral_6_decimals = effective_collateral / 100; 
    let health_factor = crate::utils::calculate_health_factor(effective_collateral_6_decimals, user_position.usdc_borrowed);
    
    // 7. Verify if health factor is below liquidation threshold
    require!(
        health_factor <= protocol_config.liquidation_threshold as u64,
        LendingError::PositionNotLiquidatable
    );
    
    // 8. Calculate liquidatable debt amount (for logging only)
    let liquidatable_debt = calculate_liquidatable_debt(
        user_position.usdc_borrowed,
        health_factor,
        protocol_config.liquidation_threshold as u64,
        10000, 
    )?;

    // Liquidator does not need to pay any fee, as they will use the LP tokens of the liquidated user to remove liquidity and get the USDC back
    // Only verify that the liquidator account exists, no need to check balance
    
    // 9. Verify protocol status
    require!(
        !protocol_config.is_paused,
        LendingError::ProtocolPaused
    );
    
    // 10. Verify LP NFT status - ensure NFT mint is valid and PersonalPosition data exists
    // We have already verified that PersonalPosition data is valid, so the NFT is valid
    require!(
        position_info.liquidity > 0,
        LendingError::InvalidLpTokenState
    );
    
    
    
    Ok((health_factor, current_lp_value))
}


/// Real-time LP value calculation - directly call the complete calculate_clmm_position_value in valuation.rs
fn calculate_real_time_lp_value(
    ctx: &Context<TriggerLiquidation>,
    user_position: &UserPosition,
    _sol_price: u64, 
) -> Result<(u64, Pubkey, PersonalPositionInfo)> {
    
    
    // 1. Get the real NFT mint address
    let nft_mint = user_position.lp_mint;
    require!(nft_mint != Pubkey::default(), LendingError::InvalidLpMint);
    
    
    // 2. Verify PersonalPosition address matches NFT mint
    let raydium_clmm_program = anchor_lang::solana_program::pubkey!("DRayAUgENGQBKVaX8owNhgzkEDyoHTGVEGHVJT1E9pfH");
    let (expected_position_addr, _position_bump) = Pubkey::find_program_address(
        &[
            b"position",
            nft_mint.as_ref(),
        ],
        &raydium_clmm_program
    );
    
    // ❌ verify that PersonalPosition address matches expected address derived from NFT mint
    require_eq!(
        ctx.accounts.personal_position.key(),
        expected_position_addr,
        LendingError::InvalidPersonalPosition
    );
    
    
    // 3. Directly call the function in valuation.rs
    
    let sol_price = get_sol_price(
        &ctx.accounts.chainlink_program,
        &ctx.accounts.chainlink_sol_feed
    )?;
    
    
    // 4. Parse PersonalPosition data and calculate CLMM value
    let personal_position_data = ctx.accounts.personal_position.try_borrow_data()?;
    if personal_position_data.len() == 0 {
        return Err(LendingError::InvalidAccount.into());
    }
    
    // Parse PersonalPosition data to get liquidity information
    let position_info = parse_personal_position_data(&personal_position_data)?;
    
    
    // 5. Parse CLMM Pool current tick
    let clmm_pool_data = ctx.accounts.clmm_pool_state.try_borrow_data()?;
    let current_tick = parse_clmm_current_tick(&clmm_pool_data)?;
    
    
    // 6. Calculate token amounts based on liquidity
    let (sol_amount, usdc_amount) = calculate_token_amounts_from_liquidity(
        position_info.liquidity,
        current_tick,
        position_info.tick_lower,
        position_info.tick_upper,
    )?;
    
    
    // 7. Calculate total CLMM position value
    let usdc_price = 1_000_000u64; // $1.000000
    let clmm_position_value = calculate_value_from_token_amounts(
        sol_amount,
        usdc_amount,
        sol_price,
        usdc_price,
    )?;
    
    
    // 8. Verify CLMM position value is positive
    require!(clmm_position_value > 0, LendingError::ZeroAmount);
    
    // 9. Verify CLMM position value is sufficient
    let min_clmm_value = 100_000_000u64; // $1.00 
    if clmm_position_value < min_clmm_value {
        return Err(LendingError::InsufficientCollateral.into());
    }
    
    
    Ok((clmm_position_value, nft_mint, position_info))
}


#[deprecated(note = "Use utils::apply_discount instead")]
pub fn apply_discount_factor(_lp_value: u64, _discount_factor: u16) -> Result<u64> {
    Err(LendingError::DeprecatedFunction.into())
}

#[deprecated(note = "Use utils::calculate_health_factor instead")]
pub fn calculate_health_factor(_collateral_value: u64, _debt_amount: u64) -> Result<u64> {
    Err(LendingError::DeprecatedFunction.into())
}

/// Verify if a user position is eligible for liquidation based on health factor and protocol config
pub fn validate_liquidation_eligibility(
    user_position: &UserPosition,
    protocol_config: &ProtocolConfig,
) -> Result<bool> {
    // Check if user has any debt
    if !user_position.has_debt() {
        return Ok(false);
    }
    
    // Check if user is already pending liquidation
    if user_position.liquidation_pending {
        return Ok(false);
    }
    
    // Check health factor against liquidation threshold
    let health_factor = user_position.health_factor;
    let liquidation_threshold = protocol_config.liquidation_threshold as u64;
    
    if health_factor >= liquidation_threshold {
        return Ok(false);
    }
    
    // Check if user has any LP deposited
    if user_position.lp_deposited == 0 {
        return Ok(false);
    }
    
    
    Ok(true)
}

/// Calculate the amount of liquidatable debt based on health factor
/// According to the requirement document: Health factor ≤ 110% for full liquidation (repay all debt)
pub fn calculate_liquidatable_debt(
    total_debt: u64,
    health_factor: u64,
    liquidation_threshold: u64,
    _max_liquidation_ratio: u16, // Reserved parameter for compatibility, not used
) -> Result<u64> {
    if health_factor >= liquidation_threshold {
        return Ok(0); // Health factor is good, no need to liquidate
    }
    
    // According to the requirement document:
    // Health factor ≤ 110% for full liquidation (repay all debt)
    let liquidatable_debt = total_debt; 
    
    
    Ok(liquidatable_debt)
}

