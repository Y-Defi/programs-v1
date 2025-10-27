use anchor_lang::prelude::*;
use chainlink_solana as chainlink;
use crate::errors::LendingError;

/// Raydium CLMM pool state structure
/// Raydium CLMM pool state structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct RaydiumClmmInfo {

    pub version: u8,
    
    pub status: u8,
    
    pub observation_index: u16,
    
    pub observation_cardinality: u16,
    
    pub observation_cardinality_next: u16,
    
    pub fee_rate: u32,
    
    pub protocol_fee_rate: u32,
    
    pub liquidity: u128,
    
    pub sqrt_price_x64: u128,
    
    pub tick_current: i32,
    
    pub padding0: [u8; 4],
    
    pub fee_growth_global_0_x64: u128,
      
    pub fee_growth_global_1_x64: u128,
    
    pub protocol_fees_token_0: u64,
    
    pub protocol_fees_token_1: u64,
    
    pub swap_in_amount_token_0: u128,
    
    pub swap_out_amount_token_1: u128,
   
    pub swap_in_amount_token_1: u128,
    
    pub swap_out_amount_token_0: u128,
    
    pub token_mint_0: Pubkey,
   
    pub token_mint_1: Pubkey,
    
    pub token_vault_0: Pubkey,
    
    pub token_vault_1: Pubkey,
    
    pub observations: [u8; 1280], 
}


impl RaydiumClmmInfo {
    pub const LEN: usize = 1544; 
    
    /// Parse Raydium CLMM info from account data
    pub fn from_account_data(data: &[u8]) -> Result<Self> {
        require!(data.len() >= Self::LEN, LendingError::InvalidPoolData);
        
        // Simplified CLMM data parsing - only parse key fields
        let clmm_info = RaydiumClmmInfo {
            version: data[0],
            status: data[1], 
            observation_index: u16::from_le_bytes(data[2..4].try_into().unwrap()),
            observation_cardinality: u16::from_le_bytes(data[4..6].try_into().unwrap()),
            observation_cardinality_next: u16::from_le_bytes(data[6..8].try_into().unwrap()),
            fee_rate: u32::from_le_bytes(data[8..12].try_into().unwrap()),
            protocol_fee_rate: u32::from_le_bytes(data[12..16].try_into().unwrap()),
            liquidity: u128::from_le_bytes(data[16..32].try_into().unwrap()),
            sqrt_price_x64: u128::from_le_bytes(data[32..48].try_into().unwrap()),
            tick_current: i32::from_le_bytes(data[48..52].try_into().unwrap()),
            padding0: [0; 4],
            fee_growth_global_0_x64: u128::from_le_bytes(data[56..72].try_into().unwrap()),
            fee_growth_global_1_x64: u128::from_le_bytes(data[72..88].try_into().unwrap()),
            protocol_fees_token_0: u64::from_le_bytes(data[88..96].try_into().unwrap()),
            protocol_fees_token_1: u64::from_le_bytes(data[96..104].try_into().unwrap()),
            swap_in_amount_token_0: u128::from_le_bytes(data[104..120].try_into().unwrap()),
            swap_out_amount_token_1: u128::from_le_bytes(data[120..136].try_into().unwrap()),
            swap_in_amount_token_1: u128::from_le_bytes(data[136..152].try_into().unwrap()),
            swap_out_amount_token_0: u128::from_le_bytes(data[152..168].try_into().unwrap()),
            token_mint_0: Pubkey::new_from_array(data[168..200].try_into().unwrap()),
            token_mint_1: Pubkey::new_from_array(data[200..232].try_into().unwrap()),
            token_vault_0: Pubkey::new_from_array(data[232..264].try_into().unwrap()),
            token_vault_1: Pubkey::new_from_array(data[264..296].try_into().unwrap()),
            observations: [0; 1280],
        };

        // Validate pool status
        require!(clmm_info.liquidity > 0, LendingError::InvalidPoolData);
        require!(clmm_info.sqrt_price_x64 > 0, LendingError::InvalidPoolData);

        Ok(clmm_info)
    }
    
    /// Calculate value of a concentrated liquidity position
    /// Based on current price and liquidity, calculate amounts of both tokens
    pub fn calculate_clmm_position_value(
        &self,
        position_liquidity: u128,
        tick_lower: i32,
        tick_upper: i32,
        token_0_price_usd: u64, 
        token_1_price_usd: u64, 
    ) -> Result<u64> {
        require!(position_liquidity > 0, LendingError::ZeroAmount);
        require!(tick_lower < tick_upper, LendingError::InvalidPoolData);
        
        // Calculate amounts of both tokens in the position liquidity
        let (amount_0, amount_1) = self.get_amounts_from_liquidity(
            position_liquidity,
            tick_lower,
            tick_upper,
        )?;
        
        // Calculate value of token 0 in USD (assuming SOL, 9 decimal precision)
        let token_0_value_usd = (amount_0 as u128)
            .checked_mul(token_0_price_usd as u128)
            .and_then(|v| v.checked_div(1_000_000_000u128)) // SOL精度调整
            .ok_or(LendingError::MathOverflow)? as u64;
        
        // Calculate value of token 1 in USD (assuming USDC, 6 decimal precision) - return micro-USD units
        // Convert amount_1 from micro-USD to USD by dividing by 1,000,000
        let token_1_value_usd = (amount_1 as u128)
            .checked_mul(token_1_price_usd as u128)
            .and_then(|v| v.checked_div(1_000_000u128)) 
            .ok_or(LendingError::MathOverflow)? as u64;
        
        let total_value_usd = token_0_value_usd
            .checked_add(token_1_value_usd)
            .ok_or(LendingError::MathOverflow)?;

        Ok(total_value_usd)
    }
    
    /// Calculate amounts of both tokens in the position liquidity
    /// This is core math formula for concentrated liquidity positions
    fn get_amounts_from_liquidity(
        &self,
        liquidity: u128,
        tick_lower: i32,
        tick_upper: i32,
    ) -> Result<(u64, u64)> {
        let current_tick = self.tick_current;
        
        if current_tick <= tick_lower {
            // Price is lower than the range, token0
            let amount_0 = self.calculate_amount_0_from_liquidity(liquidity, tick_lower, tick_upper)?;
            Ok((amount_0, 0))
        } else if current_tick >= tick_upper {
            // Price is higher than the range, token1
            let amount_1 = self.calculate_amount_1_from_liquidity(liquidity, tick_lower, tick_upper)?;
            Ok((0, amount_1))
        } else {
            // Price is in the range, both tokens have liquidity
            let amount_0 = self.calculate_amount_0_from_liquidity(liquidity, current_tick, tick_upper)?;
            let amount_1 = self.calculate_amount_1_from_liquidity(liquidity, tick_lower, current_tick)?;
            Ok((amount_0, amount_1))
        }
    }
    
    /// calculate amount of token0 in the position liquidity
    fn calculate_amount_0_from_liquidity(
        &self,
        liquidity: u128,
        tick_a: i32,
        tick_b: i32,
    ) -> Result<u64> {
        // Simplified formula: liquidity * (tick_b - tick_a) / 100000
        let tick_diff = (tick_b - tick_a).abs() as u128;
        let amount = liquidity
            .checked_div(1000) 
            .and_then(|v| v.checked_mul(tick_diff))
            .and_then(|v| v.checked_div(100000))
            .ok_or(LendingError::MathOverflow)?;
            
        Ok(amount.min(u64::MAX as u128) as u64)
    }
    
    /// calculate amount of token1 in the position liquidity
    fn calculate_amount_1_from_liquidity(
        &self,
        liquidity: u128,
        tick_a: i32,
        tick_b: i32,
    ) -> Result<u64> {
       
        let tick_diff = (tick_b - tick_a).abs() as u128;
        let amount = liquidity
            .checked_div(1000) 
            .and_then(|v| v.checked_mul(tick_diff))
            .and_then(|v| v.checked_div(100000))
            .ok_or(LendingError::MathOverflow)?;
            
        Ok(amount.min(u64::MAX as u128) as u64)
    }
    
    /// Get basic info of pool
    pub fn get_pool_info(&self) -> (u128, u128, i32) {
        (self.liquidity, self.sqrt_price_x64, self.tick_current)
    }
    
    /// Check if token mints match the pool's configuration
    pub fn verify_token_mints(&self, token_0_mint: &Pubkey, token_1_mint: &Pubkey) -> bool {
        (self.token_mint_0 == *token_0_mint && self.token_mint_1 == *token_1_mint) ||
        (self.token_mint_0 == *token_1_mint && self.token_mint_1 == *token_0_mint)
    }
}

/// Chainlink 价格数据结构
/// Chainlink price data structure
pub struct ChainlinkPriceData {
    pub price: i128,      // Chainlink price
    pub decimals: u8,     
    pub description: String, 
    pub timestamp: u64,   
}

impl ChainlinkPriceData {

    pub fn to_scaled_price(&self) -> Result<u64> {
        if self.price <= 0 {
            return Ok(0);
        }
        
        let price_abs = self.price as u128;
        
        let scaled_price = match self.decimals {
            8 => price_abs / 100,      
            6 => price_abs,           
            d if d < 6 => price_abs * (10u128.pow(6 - d as u32)), 
            d if d > 6 => price_abs / (10u128.pow(d as u32 - 6)), 
            _ => price_abs,
        };
        
        if scaled_price > u64::MAX as u128 {
            return Err(LendingError::MathOverflow.into());
        }
        
        Ok(scaled_price as u64)
    }
    
    pub fn is_fresh(&self) -> bool {
        let current_slot = Clock::get().unwrap().slot;
        current_slot.saturating_sub(self.timestamp) < 750
    }
}

pub fn get_raydium_pool_data(pool_account: &AccountInfo) -> Result<RaydiumClmmInfo> {
    let pool_data = pool_account.try_borrow_data()?;

    if pool_data.len() == RaydiumClmmInfo::LEN {
        let clmm_info = RaydiumClmmInfo::from_account_data(&pool_data)?;

        Ok(clmm_info)
    } else {
        Err(LendingError::InvalidPoolData.into())
    }
}

pub fn get_raydium_clmm_data(pool_account: &AccountInfo) -> Result<RaydiumClmmInfo> {
    let pool_data = pool_account.try_borrow_data()?;
    let clmm_info = RaydiumClmmInfo::from_account_data(&pool_data)?;

    Ok(clmm_info)
}

pub fn get_chainlink_sol_price<'a>(
    chainlink_program: &AccountInfo<'a>,
    sol_usd_feed: &AccountInfo<'a>,
) -> Result<ChainlinkPriceData> {
    let round: chainlink::Round = chainlink::latest_round_data(
        chainlink_program.clone(),
        sol_usd_feed.clone(),
    )?;
    
    let description: String = chainlink::description(
        chainlink_program.clone(),
        sol_usd_feed.clone(),
    )?;
    
    let decimals: u8 = chainlink::decimals(
        chainlink_program.clone(),
        sol_usd_feed.clone(),
    )?;
    
    require!(round.answer > 0, LendingError::InvalidPrice);
    
    let price_data = ChainlinkPriceData {
        price: round.answer,
        decimals,
        description: description.clone(),
        timestamp: round.timestamp as u64,
    };
    
    require!(price_data.is_fresh(), LendingError::StalePrice);

    Ok(price_data)
}

#[derive(Debug, Clone)]
pub struct PositionData {
    pub liquidity: u128,
    pub tick_lower: i32,
    pub tick_upper: i32,
    pub token_fees_owed_0: u64,
    pub token_fees_owed_1: u64,
}

pub struct RaydiumIntegration;

impl RaydiumIntegration {
    pub fn new() -> Self {
        Self
    }
    
    pub fn detect_pool_type(&self, pool_account: &AccountInfo) -> Result<RaydiumClmmInfo> {
        get_raydium_pool_data(pool_account)
    }
    
    pub fn parse_position_nft(&self, position_account: &AccountInfo) -> Result<PositionData> {
        let position_data = position_account.try_borrow_data()?;
        
        if position_data.len() < 216 { 
            return Err(LendingError::InvalidPositionNft.into());
        }
        
        
        let pool_id = Pubkey::try_from(&position_data[8..40])
            .map_err(|_| LendingError::InvalidPositionNft)?;
            
        let liquidity_bytes: [u8; 16] = position_data[72..88].try_into()
            .map_err(|_| LendingError::InvalidPositionNft)?;
        let liquidity = u128::from_le_bytes(liquidity_bytes);
        
        let tick_lower_bytes: [u8; 4] = position_data[88..92].try_into()
            .map_err(|_| LendingError::InvalidPositionNft)?;
        let tick_lower = i32::from_le_bytes(tick_lower_bytes);
        
        let tick_upper_bytes: [u8; 4] = position_data[92..96].try_into()
            .map_err(|_| LendingError::InvalidPositionNft)?;
        let tick_upper = i32::from_le_bytes(tick_upper_bytes);
        
        let fees_0_bytes: [u8; 8] = position_data[96..104].try_into()
            .map_err(|_| LendingError::InvalidPositionNft)?;
        let token_fees_owed_0 = u64::from_le_bytes(fees_0_bytes);
        
        let fees_1_bytes: [u8; 8] = position_data[104..112].try_into()
            .map_err(|_| LendingError::InvalidPositionNft)?;
        let token_fees_owed_1 = u64::from_le_bytes(fees_1_bytes);
        
        if liquidity == 0 {
            return Err(LendingError::InvalidPositionNft.into());
        }
        
        if tick_lower >= tick_upper {
            return Err(LendingError::InvalidPositionNft.into());
        }
        
        let position_info = PositionData {
            liquidity,
            tick_lower,
            tick_upper,
            token_fees_owed_0,
            token_fees_owed_1,
        };

        Ok(position_info)
    }
    
    pub fn calculate_clmm_position_value(
        &self,
        clmm_info: &RaydiumClmmInfo,
        position_liquidity: u128,
        tick_lower: i32,
        tick_upper: i32,
        token_0_price_usd: u64,
        token_1_price_usd: u64,
    ) -> Result<u64> {
        let sqrt_price_current = clmm_info.sqrt_price_x64;
        let current_tick = clmm_info.tick_current;
        
        let sqrt_price_lower = self.get_sqrt_ratio_at_tick(tick_lower)?;
        let sqrt_price_upper = self.get_sqrt_ratio_at_tick(tick_upper)?;

        let (amount_0, amount_1) = self.get_amounts_for_liquidity(
            sqrt_price_current,
            sqrt_price_lower,
            sqrt_price_upper,
            position_liquidity,
        )?;
        
        let actual_amount_0 = amount_0; 
        let actual_amount_1 = amount_1; 
        
        let value_0_scaled = (actual_amount_0 as u128)
            .checked_mul(token_0_price_usd as u128)
            .and_then(|v| v.checked_div(1_000_000_000)) 
            .ok_or(LendingError::MathOverflow)?;
            
        let value_1_scaled = (actual_amount_1 as u128)
            .checked_mul(token_1_price_usd as u128) 
            .and_then(|v| v.checked_div(1_000_000)) 
            .ok_or(LendingError::MathOverflow)?;
        
        let total_value = value_0_scaled
            .checked_add(value_1_scaled)
            .ok_or(LendingError::MathOverflow)?;
        
        let final_value = total_value.min(u64::MAX as u128) as u64;
        
        
        Ok(final_value)
    }
    
    fn get_sqrt_ratio_at_tick(&self, tick: i32) -> Result<u128> {
        // Simplified implementation: use approximate formula to avoid complex fixed-point arithmetic
        // Original Formular: sqrt_price = sqrt(1.0001^tick)
        // Simplified to linear approximation: sqrt_price ≈ base_price * (1 + tick * 0.00005)
        

        let base_price: u128 = 1u128 << 64;
        
        let tick_factor = if tick >= 0 {
            base_price + (tick as u128 * base_price / 100000) 
        } else {
            let tick_abs = (-tick) as u128;
            let price_decrease = tick_abs * base_price / 100000;
            if price_decrease >= base_price {
                base_price / 1000 
            } else {
                base_price - price_decrease
            }
        };
        
        let sqrt_price = self.integer_sqrt(tick_factor)?;
        
        Ok(sqrt_price)
    }
    
    fn integer_sqrt(&self, value: u128) -> Result<u128> {
        if value == 0 {
            return Ok(0);
        }
        
        // Newton Method
        let mut x = value;
        let mut y = (x + 1) / 2;
        
        while y < x {
            x = y;
            y = (x + value / x) / 2;
        }
        
        Ok(x)
    }
    

    fn get_amounts_for_liquidity(
        &self,
        sqrt_price_current: u128,
        sqrt_price_lower: u128,
        sqrt_price_upper: u128,
        liquidity: u128,
    ) -> Result<(u64, u64)> {
        let mut amount_0: u128 = 0;
        let mut amount_1: u128 = 0;
        
        if sqrt_price_current <= sqrt_price_lower {
            amount_0 = self.get_amount_0_delta(
                sqrt_price_lower,
                sqrt_price_upper,
                liquidity,
            )?;
        } else if sqrt_price_current >= sqrt_price_upper {
            amount_1 = self.get_amount_1_delta(
                sqrt_price_lower,
                sqrt_price_upper,
                liquidity,
            )?;
        } else {
            amount_0 = self.get_amount_0_delta(
                sqrt_price_current,
                sqrt_price_upper,
                liquidity,
            )?;
            
            amount_1 = self.get_amount_1_delta(
                sqrt_price_lower,
                sqrt_price_current,
                liquidity,
            )?;
        }
        
        let final_amount_0 = amount_0.min(u64::MAX as u128) as u64;
        let final_amount_1 = amount_1.min(u64::MAX as u128) as u64;
        
        Ok((final_amount_0, final_amount_1))
    }
    
    fn get_amount_0_delta(
        &self,
        sqrt_ratio_a: u128,
        sqrt_ratio_b: u128,
        liquidity: u128,
    ) -> Result<u128> {
        if sqrt_ratio_a > sqrt_ratio_b {
            return self.get_amount_0_delta(sqrt_ratio_b, sqrt_ratio_a, liquidity);
        }
        
        // amount0 = liquidity * (sqrt_ratio_b - sqrt_ratio_a) / (sqrt_ratio_a * sqrt_ratio_b)
        let numerator_1 = liquidity << 96; 
        let numerator_2 = sqrt_ratio_b - sqrt_ratio_a;
        
        let denominator = sqrt_ratio_a
            .checked_mul(sqrt_ratio_b)
            .ok_or(LendingError::MathOverflow)?
            >> 96; 
        
        let amount = numerator_1
            .checked_mul(numerator_2)
            .and_then(|v| v.checked_div(denominator))
            .and_then(|v| v.checked_div(1 << 96)) 
            .ok_or(LendingError::MathOverflow)?;
        
        Ok(amount)
    }
    
    fn get_amount_1_delta(
        &self,
        sqrt_ratio_a: u128,
        sqrt_ratio_b: u128,
        liquidity: u128,
    ) -> Result<u128> {
        if sqrt_ratio_a > sqrt_ratio_b {
            return self.get_amount_1_delta(sqrt_ratio_b, sqrt_ratio_a, liquidity);
        }
        
        // amount1 = liquidity * (sqrt_ratio_b - sqrt_ratio_a)
        let amount = liquidity
            .checked_mul(sqrt_ratio_b - sqrt_ratio_a)
            .and_then(|v| v.checked_div(1 << 96)) 
            .ok_or(LendingError::MathOverflow)?;
        
        Ok(amount)
    }
}