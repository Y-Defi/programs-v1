// Math Module - LP Value Calculator and Health Ratio Calculator
use anchor_lang::prelude::*;
use crate::errors::LendingError;

/// Math Constants
pub struct MathConstants;

impl MathConstants {
    /// Basic precision (10000 = 100%)
    pub const BASIS_POINTS: u64 = 10_000;
    /// USDC precision (6 decimal places)
    pub const USDC_DECIMALS: u8 = 6;
    /// SOL precision (9 decimal places)
    pub const SOL_DECIMALS: u8 = 9;
    /// Pyth price precision (usually 8 decimal places)
    pub const PRICE_DECIMALS: u8 = 8;
    /// Maximum health ratio threshold (to prevent overflow)
    pub const MAX_HEALTH_RATIO: u64 = 1_000_000; // 10000%
}

/// LP Token Value Calculator
pub struct LpValueCalculator;

impl LpValueCalculator {
    /// Calculate USDC value of LP tokens
    /// Formula: (sol_reserve * sol_price + usdc_reserve) / lp_supply * user_lp_amount
    pub fn calculate_lp_value_in_usdc(
        user_lp_amount: u64,
        sol_reserve: u64,         // SOL reserve amount (9 decimal places)
        usdc_reserve: u64,        // USDC reserve amount (6 decimal places)
        lp_total_supply: u64,     // Total LP token supply
        sol_price_usd: u64,       // SOL price in USD (8 decimal places)
    ) -> Result<u64> {
        if lp_total_supply == 0 {
            return err!(LendingError::InvalidPoolState);
        }
        if user_lp_amount == 0 {
            return Ok(0);
        }

        // Calculate USDC value of SOL part
        // sol_reserve (9 decimal places) * sol_price (8 decimal places) / 10^8 = sol_value_usdc (9 decimal places)
        let sol_value_usdc = Self::safe_mul_div(
            sol_reserve,
            sol_price_usd,
            10u64.pow(MathConstants::PRICE_DECIMALS as u32),
        )?;

        // Adjust SOL value precision to USDC precision (9 decimal places -> 6 decimal places)
        let sol_value_usdc = sol_value_usdc
            .checked_div(10u64.pow((MathConstants::SOL_DECIMALS - MathConstants::USDC_DECIMALS) as u32))
            .ok_or(LendingError::MathOverflow)?;

        // Calculate total pool value in USDC precision (9 decimal places)
        let total_pool_value = sol_value_usdc
            .checked_add(usdc_reserve)
            .ok_or(LendingError::MathOverflow)?;

        // Calculate USDC value of LP tokens
        let user_lp_value = Self::safe_mul_div(
            total_pool_value,
            user_lp_amount,
            lp_total_supply,
        )?;

        Ok(user_lp_value)
    }

    /// Safe multiplication and division to prevent overflow
    fn safe_mul_div(a: u64, b: u64, c: u64) -> Result<u64> {
        if c == 0 {
            return err!(LendingError::MathOverflow);
        }

        // Use u128 to prevent intermediate overflow
        let result = (a as u128)
            .checked_mul(b as u128)
            .ok_or(LendingError::MathOverflow)?
            .checked_div(c as u128)
            .ok_or(LendingError::MathOverflow)?;

        if result > u64::MAX as u128 {
            return err!(LendingError::MathOverflow);
        }

        Ok(result as u64)
    }
}

/// Risk Calculator
pub struct RiskCalculator;

impl RiskCalculator {
    /// Calculate borrow limit
    /// Formula: lp_value_usdc * discount_rate * collateral_ratio / 10000^2
    pub fn calculate_borrow_limit(
        lp_value_usdc: u64,
        discount_rate: u16,      // 8000 = 80%
        collateral_ratio: u16,   // 5000 = 50%
    ) -> Result<u64> {
        if lp_value_usdc == 0 {
            return Ok(0);
        }

        let borrow_limit = LpValueCalculator::safe_mul_div(
            lp_value_usdc,
            (discount_rate as u64) * (collateral_ratio as u64),
            MathConstants::BASIS_POINTS * MathConstants::BASIS_POINTS,
        )?;

        Ok(borrow_limit)
    }

    /// Calculate health ratio
    /// Formula: collateral_value_usdc / debt_value_usdc * 100
    /// Returns: Health ratio percentage (120 = 120%)
    pub fn calculate_health_ratio(
        collateral_value_usdc: u64,
        debt_value_usdc: u64,
    ) -> Result<u64> {
        if debt_value_usdc == 0 {
            return Ok(MathConstants::MAX_HEALTH_RATIO); // Health ratio is max when no debt
        }
        if collateral_value_usdc == 0 {
            return Ok(0); // Health ratio is 0 when no collateral
        }

        let health_ratio = LpValueCalculator::safe_mul_div(
            collateral_value_usdc,
            100,
            debt_value_usdc,
        )?;

        // Limit max health ratio to prevent overflow
        Ok(health_ratio.min(MathConstants::MAX_HEALTH_RATIO))
    }

    /// Check if position is liquidatable
    pub fn is_liquidatable(health_ratio: u64, liquidation_threshold: u16) -> bool {
        health_ratio <= liquidation_threshold as u64
    }

    /// Check if position is healthy (for warnings)
    pub fn get_health_status(health_ratio: u64) -> HealthStatus {
        if health_ratio <= 110 {
            HealthStatus::Critical  // Critical warning (<=110%)
        } else if health_ratio <= 120 {
            HealthStatus::Warning   // Warning (110-120%)
        } else {
            HealthStatus::Healthy   // Healthy (>120%)
        }
    }

    /// Calculate LP amount needed for liquidation
    /// Target: Health ratio just below safe threshold (e.g. 150%)
    pub fn calculate_liquidation_amount(
        lp_deposited: u64,
        debt_value: u64,
        lp_unit_value: u64,
        target_health_ratio: u64, // 150 = 150%
    ) -> Result<u64> {
        if debt_value == 0 || lp_unit_value == 0 {
            return Ok(0);
        }

        // Calculate required collateral value to achieve target health ratio
        let required_collateral = LpValueCalculator::safe_mul_div(
            debt_value,
            target_health_ratio,
            100,
        )?;

        let current_collateral = LpValueCalculator::safe_mul_div(
            lp_deposited,
            lp_unit_value,
            1,
        )?;

        if current_collateral <= required_collateral {
            // Need to liquidate all LP tokens
            return Ok(lp_deposited);
        }

        // Calculate required LP tokens for liquidation
        let liquidation_value = current_collateral
            .checked_sub(required_collateral)
            .ok_or(LendingError::MathOverflow)?;

        // Convert to LP tokens
        let liquidation_lp = LpValueCalculator::safe_mul_div(
            liquidation_value,
            1,
            lp_unit_value,
        )?;

        Ok(liquidation_lp.min(lp_deposited))
    }
}

/// Health status enum
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HealthStatus {
    Healthy,   // 健康 (>120%)
    Warning,   // 警告 (110-120%)
    Critical,  // 危险 (≤110%)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lp_value_calculation() {
        // Test LP value calculation
        // Assume: 1000 SOL, 50000 USDC, total LP: 1000, SOL price: $50
        let result = LpValueCalculator::calculate_lp_value_in_usdc(
            100, // User holds 100 LP tokens
            1000 * 10u64.pow(9), // 1000 SOL
            50000 * 10u64.pow(6), // 50000 USDC
            1000, // Total LP supply: 1000
            50 * 10u64.pow(8), // $50/SOL
        ).unwrap();

        // Expected result: (1000*50 + 50000) / 1000 * 100 = 10000 USDC
        assert_eq!(result, 10000 * 10u64.pow(6));
    }

    #[test]
    fn test_health_ratio_calculation() {
        // Test health ratio calculation
        let health = RiskCalculator::calculate_health_ratio(
            15000 * 10u64.pow(6), // $15000 collateral
            10000 * 10u64.pow(6), // $10000 debt
        ).unwrap();

        assert_eq!(health, 150); // 150%
    }

    #[test]
    fn test_borrow_limit_calculation() {
        // Test borrow limit calculation
        let limit = RiskCalculator::calculate_borrow_limit(
            10000 * 10u64.pow(6), // $10000 LP value
            8000, // 80% discount rate
            5000, // 50% collateral ratio
        ).unwrap();

        // Expected result: 10000 * 0.8 * 0.5 = 4000 USDC
        assert_eq!(limit, 4000 * 10u64.pow(6));
    }
}