
use anchor_lang::prelude::*;

pub mod state;
pub mod instructions; 
pub mod errors;
pub mod utils;
pub mod math;

use instructions::*;

declare_id!("2aApaTirAuGd5CrcP88tPA3HQR9MyqKL67z1GRcBeAtV");

#[program]
pub mod solana_lp_lending {
    use super::*;

    /// Protocol initialization - set core parameters and risk control parameters
    pub fn initialize_protocol(
        ctx: Context<InitializeProtocol>,
        collateral_ratio: u16,
        liquidation_threshold: u16,
        liquidation_executor: Pubkey,
    ) -> Result<()> {
        instructions::initialize_protocol::handler(ctx, collateral_ratio, liquidation_threshold, liquidation_executor) }

    /// Pause/Resume protocol operation status
    /// Parameters:
    /// - ctx: Context for protocol pause/resume operation
    pub fn toggle_protocol(ctx: Context<ToggleProtocol>) -> Result<()> {
        instructions::initialize_protocol::toggle_protocol_handler(ctx)
    }

    /// Update risk control parameters
    pub fn update_risk_parameters(
        ctx: Context<UpdateRiskParameters>,
        new_collateral_ratio: Option<u16>,
        new_liquidation_threshold: Option<u16>,
    ) -> Result<()> {
        instructions::initialize_protocol::update_risk_parameters_handler(
            ctx,
            new_collateral_ratio,
            new_liquidation_threshold
        )
    }

    /// LP token deposit operation
    pub fn deposit_lp(ctx: Context<DepositLP>, lp_mint_param: Pubkey, amount: u64) -> Result<()> {
        instructions::lp_operations::deposit_lp(ctx, lp_mint_param, amount)
    }

    /// LP token withdraw operation
    pub fn withdraw_lp(ctx: Context<WithdrawLP>, amount: u64) -> Result<()> {
        instructions::lp_operations::withdraw_lp(ctx, amount)
    }

    /// Emergency LP token withdraw operation (available when protocol is paused)
    pub fn emergency_withdraw(ctx: Context<EmergencyWithdraw>) -> Result<()> {
        instructions::lp_operations::emergency_withdraw(ctx)
    }

    /// USDC borrow operation
    pub fn borrow_usdc(ctx: Context<BorrowUSDC>, lp_mint: Pubkey, amount: u64) -> Result<()> {
        instructions::borrow_operations::borrow_usdc(ctx, lp_mint, amount)
    }

    /// USDC repay operation
    pub fn repay_usdc(ctx: Context<RepayUSDC>, amount: u64) -> Result<()> {
        instructions::borrow_operations::repay_usdc(ctx, amount)
    }

    /// Repay all borrows operation
    pub fn repay_all(ctx: Context<RepayAll>) -> Result<()> {
        instructions::borrow_operations::repay_all(ctx)
    }


    /// Update collateral value and health factor
    pub fn update_collateral_value(ctx: Context<UpdateCollateralValue>) -> Result<()> {
        instructions::valuation::update_collateral_value(ctx)
    }


    /// Check position health status
    pub fn check_health(ctx: Context<CheckHealth>) -> Result<u8> {
        instructions::valuation::check_health(ctx)
    }


    /// First step: Trigger liquidation - verify conditions and create liquidation record
    pub fn trigger_liquidation<'info>(ctx: Context<'_, '_, 'info, 'info, TriggerLiquidation<'info>>, target_user: Pubkey) -> Result<()> {
        instructions::liquidation::trigger_liquidation(ctx, target_user)
    }


    /// Third step: Complete liquidation - repay borrows and distribute rewards
    pub fn complete_liquidation<'info>(
        ctx: Context<'_, '_, '_, 'info, CompleteLiquidation<'info>>,
        debt_repaid: u64,
        user_claimable: u64,
    ) -> Result<()> {
        instructions::liquidation::complete_liquidation(ctx, debt_repaid, user_claimable)
    }


    /// Claim remaining assets after liquidation
    pub fn claim_remaining_assets<'info>(ctx: Context<'_, '_, '_, 'info, ClaimAssets<'info>>) -> Result<()> {
        instructions::liquidation::claim_remaining_assets(ctx)
    }



    /// Test get SOL price - independent oracle test function
    pub fn test_get_sol_price(ctx: Context<TestGetSolPrice>) -> Result<u64> {
        instructions::valuation::test_get_sol_price(ctx)
    }

    /// Calculate CLMM LP position value - fully on-chain parsing, only need NFT mint address
    pub fn calculate_clmm_position_value(ctx: Context<CalculateClmmPositionValue>, nft_mint: Pubkey) -> Result<u64> {
        instructions::valuation::calculate_clmm_position_value(ctx, nft_mint)
    }

    // ============ Traditional USDC Pool Instructions ============

    /// Initialize traditional USDC pool
    pub fn initialize_traditional_usdc_pool(
        ctx: Context<InitializeTraditionalUsdcPool>,
        initial_deposit_rate: u16,
        initial_borrow_rate: u16,
        min_deposit_amount: u64,
    ) -> Result<()> {
        instructions::traditional_usdc_pool::initialize_traditional_usdc_pool(
            ctx, 
            initial_deposit_rate, 
            initial_borrow_rate, 
            min_deposit_amount
        )
    }

    /// Traditional USDC pool deposit
    pub fn traditional_deposit(
        ctx: Context<TraditionalDeposit>,
        amount: u64,
        deposit_index: u32,
    ) -> Result<()> {
        instructions::traditional_usdc_pool::traditional_deposit(ctx, amount, deposit_index)
    }

    /// Traditional USDC pool withdraw
    pub fn traditional_withdraw(ctx: Context<TraditionalWithdraw>) -> Result<()> {
        instructions::traditional_usdc_pool::traditional_withdraw(ctx)
    }


    /// Update traditional USDC pool rates
    pub fn update_traditional_rates(
        ctx: Context<UpdateTraditionalRates>,
        new_deposit_rate: Option<u16>,
        new_borrow_rate: Option<u16>,
    ) -> Result<()> {
        instructions::traditional_usdc_pool::update_traditional_rates(
            ctx, 
            new_deposit_rate, 
            new_borrow_rate
        )
    }

    // ============ User Queries ============
    
    /// Get user position info
    pub fn get_user_position(ctx: Context<GetUserPosition>) -> Result<UserPositionInfo> {
        instructions::query::user_queries::get_user_position(ctx)
    }
    
    /// Get user borrow record
    pub fn get_user_borrow_record(ctx: Context<GetUserBorrowRecord>) -> Result<BorrowRecordInfo> {
        instructions::query::interest_queries::get_user_borrow_record(ctx)
    }
    
    /// Calculate user current interest
    pub fn calculate_user_interest(ctx: Context<CalculateUserInterest>) -> Result<u64> {
        instructions::query::interest_queries::calculate_user_interest(ctx)
    }
    
    /// Get protocol status info
    pub fn get_protocol_status(ctx: Context<GetProtocolStatus>) -> Result<ProtocolStatusInfo> {
        instructions::query::pool_queries::get_protocol_status(ctx)
    }
    
    /// Get pool status info
    pub fn get_pool_status(ctx: Context<GetPoolStatus>) -> Result<PoolStatusInfo> {
        instructions::query::pool_queries::get_pool_status(ctx)
    }
    
    /// Get current interest rates
    pub fn get_current_rates(ctx: Context<GetCurrentRates>) -> Result<RateInfo> {
        instructions::query::pool_queries::get_current_rates(ctx)
    }

    // pub fn get_tick_array_addresses(
    //     ctx: Context<CalculateTickArrays>,
    //     clmm_program_id: Pubkey,
    // ) -> Result<(Pubkey, Pubkey)> {
    //     instructions::liquidation::simplified_clmm::get_tick_array_addresses(ctx, clmm_program_id)
    // }
    
    /// Get user health info
    pub fn get_user_health_info(ctx: Context<GetUserHealthInfo>) -> Result<UserHealthInfo> {
        instructions::query::user_queries::get_user_health_info(ctx)
    }
    
    /// Get user all borrow records
    pub fn get_user_all_borrow_records<'info>(ctx: Context<'_, '_, 'info, 'info, GetUserAllBorrowRecords<'info>>) -> Result<UserBorrowRecords> {
        instructions::query::batch_queries::get_user_all_borrow_records(ctx)
    }
    
    /// Get user all deposit records
    pub fn get_user_all_deposit_records<'info>(ctx: Context<'_, '_, 'info, 'info, GetUserAllDepositRecords<'info>>) -> Result<UserDepositRecords> {
        instructions::query::batch_queries::get_user_all_deposit_records(ctx)
    }

    // ============ Enhanced Query Interfaces ============
    
    /// Get user comprehensive status info
    pub fn get_user_comprehensive_status<'info>(ctx: Context<'_, '_, 'info, 'info, GetUserComprehensiveStatus<'info>>) -> Result<UserComprehensiveStatus> {
        instructions::query::user_queries::get_user_comprehensive_status(ctx)
    }
    
    /// Get user basic summary info (simplified version)
    pub fn get_user_basic_summary(ctx: Context<GetUserBasicSummary>) -> Result<UserBasicSummary> {
        instructions::query::user_queries::get_user_basic_summary(ctx)
    }
    
    /// Get all users status (admin only)
    pub fn get_all_users_status<'info>(ctx: Context<'_, '_, 'info, 'info, GetAllUsersStatus<'info>>) -> Result<AllUsersStatus> {
        instructions::query::admin_queries::get_all_users_status(ctx)
    }
    
    /// Get borrow interest details
    pub fn get_borrow_interest_details(ctx: Context<GetBorrowInterestDetails>) -> Result<InterestDetails> {
        instructions::query::interest_queries::get_borrow_interest_details(ctx)
    }

    // ============ Admin Functions ============

    /// Reset user liquidation state (admin only)
    pub fn reset_liquidation_state(ctx: Context<ResetLiquidationState>) -> Result<()> {
        instructions::admin::reset_liquidation_state(ctx)
    }

    /// Close user liquidation record (admin only)
    pub fn close_liquidation_record(ctx: Context<CloseLiquidationRecord>) -> Result<()> {
        instructions::admin::close_liquidation_record(ctx)
    }

    // ============ Simulation Interfaces (Aril Frontend Support) ============

    /// Simulate borrow operation - extremely important frontend preview feature
    pub fn simulate_borrow(ctx: Context<SimulateBorrow>, amount: u64) -> Result<SimulationResult> {
        instructions::simulation::simulate_borrow(ctx, amount)
    }

    /// Simulate repay operation
    pub fn simulate_repay(ctx: Context<SimulateRepay>, amount: u64) -> Result<SimulationResult> {
        instructions::simulation::simulate_repay(ctx, amount)
    }

    /// Simulate deposit operation
    pub fn simulate_deposit(ctx: Context<SimulateDeposit>, amount: u64) -> Result<SimulateDepositResult> {
        instructions::simulation::simulate_deposit(ctx, amount)
    }

    /// Simulate withdraw operation
    pub fn simulate_withdraw(ctx: Context<SimulateWithdraw>, amount: u64) -> Result<SimulateWithdrawResult> {
        instructions::simulation::simulate_withdraw(ctx, amount)
    }

    /// Simulate supply collateral operation (add liquidity to CLMM position)
    pub fn simulate_supply_collateral(ctx: Context<SimulateSupplyCollateral>, sol_amount: u64, usdc_amount: u64) -> Result<SimulationResult> {
        instructions::simulation::simulate_supply_collateral(ctx, sol_amount, usdc_amount)
    }

}
