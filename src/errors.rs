// Set of custom error codes for the lending protocol
use anchor_lang::prelude::*;

#[error_code]
pub enum LendingError {
    #[msg("Math overflow")]
    MathOverflow,

    #[msg("Borrow amount exceeds available limit")]
    InsufficientBorrowLimit,

    #[msg("Collateral value is insufficient")]
    InsufficientCollateral,

    #[msg("User debt balance is insufficient")]
    InsufficientDebt,

    #[msg("LP token balance is insufficient")]
    InsufficientLpBalance,

    #[msg("USDC reserve is insufficient")]
    InsufficientReserve,

    #[msg("Health score is too low to perform this action")]
    UnhealthyPosition,

    #[msg("Only LP tokens can be withdrawn when debt is zero")]
    CannotWithdrawWithDebt,

    #[msg("Oracle price data is stale")]
    StaleOracleData,

    #[msg("Oracle price is invalid")]
    InvalidOraclePrice,

    #[msg("Remaining account count is insufficient")]
    InsufficientRemainingAccounts,

    #[msg("LP token mint address does not match")]
    InvalidLpMint,

    #[msg("Collateral health score is too low to perform this action")]
    PositionNotLiquidatable,

    #[msg("Liquidation reward calculation error")]
    InvalidLiquidationReward,

    #[msg("User has no claimable remaining assets")]
    NoClaimableAssets,

    #[msg("Insufficient permissions")]
    Unauthorized,

    #[msg("Invalid pool state")]
    InvalidPoolState,

    #[msg("Price slippage is too high")]
    PriceSlippageTooHigh,
    
    #[msg("Price data is stale")]
    StalePrice,

    #[msg("Operation amount is zero")]
    ZeroAmount,

    #[msg("User position account is not initialized")]
    PositionNotInitialized,

    #[msg("Global pool account is not initialized")]
    GlobalPoolNotInitialized,

    #[msg("Invalid admin authority")]
    InvalidAuthority,

    #[msg("Invalid USDC mint address")]
    InvalidUsdcMint,

    #[msg("Invalid token account owner")]
    InvalidTokenAccountOwner,

    #[msg("Invalid risk parameters")]
    InvalidRiskParameters,

    #[msg("Invalid oracle")]
    InvalidOracle,

    #[msg("Invalid price")]
    InvalidPrice,

    #[msg("Protocol is paused")]
    ProtocolPaused,

    #[msg("Protocol is not initialized")]       
    ProtocolNotInitialized,

    #[msg("Math underflow")]
    MathUnderflow,

    #[msg("Position not found")]
    PositionNotFound,

    #[msg("Excessive liquidation reward")]
    ExcessiveLiquidationReward,

    #[msg("Insufficient protocol reserves")]
    InsufficientProtocolReserves,

    #[msg("User position not found")]
    UserPositionNotFound,

    #[msg("Liquidation in progress")]
    LiquidationInProgress,

    #[msg("Insufficient deposit balance")]
    InsufficientDepositBalance,

    #[msg("Invalid pool data")]
    InvalidPoolData,

    #[msg("Outstanding debt")]
    OutstandingDebt,

    #[msg("No remaining assets")]
    NoRemainingAssets,

    #[msg("Invalid position NFT")]
    InvalidPositionNft,
    
    #[msg("Invalid account")]
    InvalidAccount,
    
    #[msg("Invalid personal position")]
    InvalidPersonalPosition,
    
    #[msg("Arithmetic overflow")]
    ArithmeticOverflow,

    // USDC pool errors
    #[msg("Insufficient liquidity")]
    InsufficientLiquidity,

    #[msg("Insufficient shares")]
    InsufficientShares,

    #[msg("Pool not initialized")]
    PoolNotInitialized,

    #[msg("User deposit not found")]
    UserDepositNotFound,

    #[msg("Share price calculation error")]
    SharePriceCalculationError,

    #[msg("Below minimum deposit")]
    BelowMinimumDeposit,

    #[msg("Below minimum withdrawal")]
    BelowMinimumWithdrawal,

    #[msg("Pool paused")]
    PoolPaused,

    #[msg("Invalid share amount")]
    InvalidShareAmount,

    #[msg("Invalid USDC amount")]
    InvalidUsdcAmount,

    // ============ Liquidation System Errors ============
    
    #[msg("Conversion rate too low")]
    ConversionRateTooLow,
    
    #[msg("Unreasonable conversion result")]
    UnreasonableConversionResult,
    
    #[msg("Insufficient conversion amount")]
    InsufficientConversionAmount,
    
    #[msg("Position not liquidatable")]
    PositionNotLiquidatable2,
    
    #[msg("Invalid LP token state")]
    InvalidLpTokenState,
    
    #[msg("Distribution mismatch")]
    DistributionMismatch,
    
    #[msg("Excessive admin fee rate")]
    ExcessiveAdminFeeRate,
    
    #[msg("Excessive liquidator reward rate")]
    ExcessiveLiquidatorRewardRate,
    
    #[msg("Excessive total fee rate")]
    ExcessiveTotalFeeRate,
    
    #[msg("Invalid distribution")]  
    InvalidDistribution,
    
    #[msg("Not in emergency state")]
    NotInEmergencyState,
    
    #[msg("Unauthorized admin")]
    UnauthorizedAdmin,
    
    #[msg("Already claimed")]
    AlreadyClaimed,
    
    #[msg("Not implemented")]
    NotImplemented,
    
    #[msg("Deprecated function")]
    DeprecatedFunction,
    
    #[msg("Use valuation module")]  
    UseValuationModule,
    
    #[msg("Invalid account data")]
    InvalidAccountData,
    
    #[msg("Slippage exceeded")]
    SlippageExceeded,
    
    #[msg("Unexpected token amount")]
    UnexpectedTokenAmount,
    
    #[msg("Insufficient balance")]
    InsufficientBalance,
    
    #[msg("Invalid program")]
    InvalidProgram,
    
    #[msg("Excessive slippage tolerance")]
    ExcessiveSlippageTolerance,
    
    #[msg("No debt")]
    NoDebt,
    
    #[msg("No collateral")]
    NoCollateral,
    
    #[msg("Liquidation pending")]
    LiquidationPending,
    
    #[msg("Invalid admin vault")]
    InvalidAdminVault,
    
    #[msg("Invalid liquidation status")]
    InvalidLiquidationStatus,
    
    #[msg("Healthy position")]
    HealthyPosition,

    // tick_array related errors
    #[msg("Invalid tick array PDA")]
    InvalidTickArrayPda,

    #[msg("Tick array account not initialized")]
    TickArrayNotInitialized,

    #[msg("Tick spacing read failed")]
    TickSpacingReadFailed,

    #[msg("Invalid repay amount")]
    InvalidRepayAmount,

    #[msg("Invalid liquidation executor")]
    InvalidLiquidationExecutor,
}