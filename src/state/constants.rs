// Modular organization of all constant values

/// Protocol seed
pub const PROTOCOL_SEED: &[u8] = b"protocol";
/// User position seed
pub const USER_POSITION_SEED: &[u8] = b"user_position";
/// LP vault seed
pub const LP_VAULT_SEED: &[u8] = b"lp_vault";
/// Liquidation record seed
pub const LIQUIDATION_SEED: &[u8] = b"liquidation";

/// Basis points (10000 = 100%)
pub const BASIS_POINTS: u128 = 10_000;
/// Price precision (1_000_000 = 6 decimal places)
pub const PRICE_PRECISION: u64 = 1_000_000;
/// SOL precision (1_000_000_000 = 9 decimal places)
pub const SOL_PRECISION: u64 = 1_000_000_000;
/// USDC precision (1_000_000 = 6 decimal places)
pub const USDC_PRECISION: u64 = 1_000_000;
/// Discount factor (80% = 8000 / 10000)
pub const DISCOUNT_FACTOR: u16 = 8000;

/// USDC pool seed  
pub const USDC_POOL_SEED: &[u8] = b"usdc_pool";
/// User deposit seed
pub const USER_DEPOSIT_SEED: &[u8] = b"user_deposit";
/// Share token mint seed
pub const SHARE_MINT_SEED: &[u8] = b"share_mint";
/// USDC pool vault seed
pub const USDC_POOL_VAULT_SEED: &[u8] = b"usdc_pool_vault";