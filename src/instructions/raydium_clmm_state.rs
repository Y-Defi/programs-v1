// Raydium CLMM PoolState Structure
use anchor_lang::prelude::*;

#[account]
#[derive(Default)]
pub struct RaydiumClmmPoolState {
    pub bump: [u8; 1],
    
    pub amm_config: Pubkey,        // 32 bytes
    pub owner: Pubkey,             // 32 bytes
    
    pub token_mint_0: Pubkey,      // 32 bytes
    pub token_mint_1: Pubkey,      // 32 bytes
    
    pub token_vault_0: Pubkey,     // 32 bytes
    pub token_vault_1: Pubkey,     // 32 bytes
    
    pub observation_key: Pubkey,   // 32 bytes
    
    pub mint_decimals_0: u8,       // 1 byte
    pub mint_decimals_1: u8,       // 1 bytes
    
    pub tick_spacing: u16,         // 2 bytes
    pub liquidity: u128,           // 16 bytes
    pub sqrt_price_x64: u128,      // 16 bytes
    pub tick_current: i32,         // 4 bytes - what we need
    
    pub tick_array_bitmap: [u64; 10], // 80 bytes
    
    pub total_fees_token_0: u64,   // 8 bytes
    pub total_fees_claimed_token_0: u64, // 8 bytes
    pub total_fees_token_1: u64,   // 8 bytes
    pub total_fees_claimed_token_1: u64, // 8 bytes
    
    pub fund_fees_token_0: u64,    // 8 bytes
    pub fund_fees_token_1: u64,    // 8 bytes
    
    pub open_time: u64,            // 8 bytes
    
}

impl RaydiumClmmPoolState {
    pub fn get_current_tick(&self) -> i32 {
        self.tick_current
    }
    
    pub fn get_sqrt_price(&self) -> u128 {
        self.sqrt_price_x64
    }
    
    pub fn get_liquidity(&self) -> u128 {
        self.liquidity
    }
}