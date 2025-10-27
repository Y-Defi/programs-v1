use anchor_lang::prelude::*;
use anchor_spl::token::TokenAccount;
use crate::errors::LendingError;
use crate::state::constants::*;
use crate::instructions::raydium_integration::get_chainlink_sol_price;

/// Utility Functions Module
/// Contains common functionalities such as mathematical operations, validations, calculations
/// Price retrieval and LP value calculations have been moved to the raydium_integration module

/// Apply discount factor to collateral value
/// Parameters:
/// - collateral_value: Original collateral value
/// - discount_factor: Discount factor (in basis points, e.g., 8000 represents 80%)
/// Returns: Value after applying the discount
pub fn apply_discount(collateral_value: u64, discount_factor: u16) -> Result<u64> {
    (collateral_value as u128)
        .checked_mul(discount_factor as u128)
        .and_then(|v| v.checked_div(BASIS_POINTS))
        .map(|v| v as u64)
        .ok_or_else(|| LendingError::MathOverflow.into())
}

/// Calculate health factor
/// Parameters:
/// - collateral_value_usd: Collateral value in USD
/// - debt_value_usd: Debt value in USD
/// Returns: Health factor (100 represents 100%, consistent with liquidation threshold in basis points)
pub fn calculate_health_factor(collateral_value_usd: u64, debt_value_usd: u64) -> u64 {
    if debt_value_usd == 0 {
        return u64::MAX; // 无债务时健康因子为最大值
    }
    
    // Calculate health factor: (collateral value / debt value) * 100
    // Note: Using 100 instead of BASIS_POINTS(10000) to match the basis point system of thresholds
    // Example: Collateral $100, debt $80, health factor = (100/80) * 100 = 125
    // Liquidation threshold 110 means 110%, health factor 125 means 125%, can be compared directly
    (collateral_value_usd as u128)
        .checked_mul(100u128)
        .and_then(|v| v.checked_div(debt_value_usd as u128))
        .map(|v| v as u64)
        .unwrap_or(0) // 如果计算溢出，返回0（最不健康）
}

/// Check position health
/// Parameters:
/// - health_factor: Health factor
/// - liquidation_threshold: Liquidation threshold (in basis points)
/// Returns: Whether the position is healthy
pub fn is_position_healthy(health_factor: u64, liquidation_threshold: u16) -> bool {
    health_factor >= liquidation_threshold as u64
}

/// Check if liquidation is possible
/// Parameters:
/// - health_factor: Health factor
/// - liquidation_threshold: Liquidation threshold (in basis points)
/// Returns: Whether liquidation is possible
pub fn is_liquidatable(health_factor: u64, liquidation_threshold: u16) -> bool {
    health_factor < liquidation_threshold as u64
}

/// Calculate maximum borrowing amount
/// Parameters:
/// - collateral_value_usd: Collateral value in USD
/// - collateral_ratio: Collateral ratio (in basis points)
/// - existing_debt: Existing debt
/// Returns: Available borrowing capacity
pub fn calculate_max_borrow_amount(
    collateral_value_usd: u64,
    collateral_ratio: u16,
    existing_debt: u64,
) -> Result<u64> {
    let max_borrow = (collateral_value_usd as u128)
        .checked_mul(collateral_ratio as u128)
        .and_then(|v| v.checked_div(BASIS_POINTS))
        .ok_or(LendingError::MathOverflow)? as u64;
    
    if max_borrow > existing_debt {
        Ok(max_borrow - existing_debt)
    } else {
        Ok(0)
    }
}

/// Verify if the token account belongs to the specified user
pub fn validate_token_account_owner(
    token_account: &Account<TokenAccount>,
    expected_owner: &Pubkey,
) -> Result<()> {
    if &token_account.owner != expected_owner {
        return Err(LendingError::InvalidTokenAccountOwner.into());
    }
    Ok(())
}

/// Verify token mint address
pub fn validate_token_mint(
    token_account: &Account<TokenAccount>,
    expected_mint: &Pubkey,
) -> Result<()> {
    if &token_account.mint != expected_mint {
        return Err(LendingError::InvalidLpMint.into());
    }
    Ok(())
}


/// Safe math operations macros
/// Used to avoid repetitive overflow checks in mathematical operations
#[macro_export]
macro_rules! safe_add {
    ($a:expr, $b:expr) => {
        $a.checked_add($b).ok_or(LendingError::MathOverflow)?
    };
}

#[macro_export]
macro_rules! safe_sub {
    ($a:expr, $b:expr) => {
        $a.checked_sub($b).ok_or(LendingError::MathOverflow)?
    };
}

#[macro_export]
macro_rules! safe_mul {
    ($a:expr, $b:expr) => {
        $a.checked_mul($b).ok_or(LendingError::MathOverflow)?
    };
}

#[macro_export]
macro_rules! safe_div {
    ($a:expr, $b:expr) => {
        $a.checked_div($b).ok_or(LendingError::MathOverflow)?
    };
}

/// Get SOL price (from Chainlink oracle)
/// Parameters:
/// - chainlink_program: Chainlink program account
/// - sol_usd_feed: SOL/USD price feed account
/// Returns: Price with 6 decimal precision
pub fn get_sol_price_chainlink<'a>(
    chainlink_program: &AccountInfo<'a>,
    sol_usd_feed: &AccountInfo<'a>,
) -> Result<u64> {
    let price_data = get_chainlink_sol_price(chainlink_program, sol_usd_feed)?;
    price_data.to_scaled_price()
}

/// Get SOL price (using Chainlink in production environment)
/// Parameters:
/// - chainlink_program: Chainlink program account  
/// - sol_usd_feed: SOL/USD price feed account
/// Returns: Price with 8 decimal precision
pub fn get_sol_price<'a>(
    chainlink_program: &AccountInfo<'a>,
    sol_usd_feed: &AccountInfo<'a>,
) -> Result<u64> {
    
    let round = chainlink_solana::latest_round_data(
        chainlink_program.to_account_info(),
        sol_usd_feed.to_account_info(),
    )?;
    
    require!(round.answer > 0, LendingError::InvalidPrice);
    
    
    Ok(round.answer as u64)
}

/// Calculate USD value of LP tokens (using Chainlink oracle)
/// Parameters:
/// - lp_amount: LP token amount
/// - raydium_pool_account: Raydium pool account
/// - chainlink_program: Chainlink program account
/// - sol_usd_feed: SOL/USD price feed account
/// Returns: Total USD value of LP tokens (6 decimal precision)
pub fn calculate_lp_value_usd<'a>(
    _lp_amount: u64,
    _raydium_pool_account: &AccountInfo,
    _chainlink_program: &AccountInfo<'a>,
    _sol_usd_feed: &AccountInfo<'a>,
) -> Result<u64> {
    // No longer supporting traditional AMM LP token calculation, only supporting CLMM
    Err(LendingError::InvalidPoolData.into())
}

/// Calculate LP value using reserve data
/// Parameters:
/// - lp_amount: LP token amount
/// - sol_reserve: SOL reserve amount
/// - usdc_reserve: USDC reserve amount
/// - lp_supply: Total LP token supply
/// - sol_price: SOL price in USD (with 8 decimal precision)
/// Returns: Total USD value of LP tokens (6 decimal precision)
pub fn calculate_lp_value_usd_with_reserves(
    lp_amount: u64,
    sol_reserve: u64,
    usdc_reserve: u64,
    lp_supply: u64,
    sol_price: u64,
) -> Result<u64> {
    if lp_supply == 0 {
        return Ok(0);
    }
    
    // Calculate SOL value (convert precision)
    let sol_value_usd = (sol_reserve as u128)
        .checked_mul(sol_price as u128)
        .and_then(|v| v.checked_div(SOL_PRECISION as u128))
        .ok_or(LendingError::MathOverflow)? as u64;
    
    // Total pool value = SOL value + USDC value
    let total_pool_value_usd = sol_value_usd
        .checked_add(usdc_reserve)
        .ok_or(LendingError::MathOverflow)?;
    
    let lp_value_usd = (lp_amount as u128)
        .checked_mul(total_pool_value_usd as u128)
        .and_then(|v| v.checked_div(lp_supply as u128))
        .ok_or(LendingError::MathOverflow)? as u64;
    
    Ok(lp_value_usd)
}