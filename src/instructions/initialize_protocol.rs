use anchor_lang::prelude::*;
use crate::state::*;
use crate::errors::LendingError;

/// Protocol initialization instruction
/// On initialization, create global protocol config and set risk parameters
#[derive(Accounts)]
pub struct InitializeProtocol<'info> {
    /// Protocol admin account, only admin can initialize protocol
    #[account(
        mut,
        constraint = authority.key() != Pubkey::default() @ LendingError::InvalidAuthority
    )]
    pub authority: Signer<'info>,
    
    /// Protocol config PDA account, stores global protocol parameters
    #[account(
        init,
        payer = authority,
        space = ProtocolConfig::LEN,
        seeds = [PROTOCOL_SEED],
        bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
    
    /// System program, used to create accounts
    pub system_program: Program<'info, System>,
}

/// Protocol initialization handler
/// On initialization, set core protocol parameters and risk controls
pub fn handler(
    ctx: Context<InitializeProtocol>,
    collateral_ratio: u16,
    liquidation_threshold: u16,
    liquidation_executor: Pubkey,
) -> Result<()> {
    // Validate risk parameter reasonableness
    require!(
        collateral_ratio > 0 && collateral_ratio <= BASIS_POINTS as u16,
        LendingError::InvalidRiskParameters
    );
    
    require!(
        liquidation_threshold > BASIS_POINTS as u16,
        LendingError::InvalidRiskParameters
    );
    
    
    // Ensure collateral ratio is below liquidation threshold, maintaining a reasonable buffer
    require!(
        collateral_ratio < liquidation_threshold,
        LendingError::InvalidRiskParameters
    );
    
    // Calculate admin fee vault PDA
    let (admin_fee_vault, _fee_vault_bump) = Pubkey::find_program_address(
        &[b"admin_fee_vault"],
        ctx.program_id
    );
    
    let protocol_config = &mut ctx.accounts.protocol_config;
    
    // Validate liquidation executor is not default
    require!(
        liquidation_executor != Pubkey::default(),
        LendingError::InvalidAuthority
    );

    // Set protocol configuration parameters - basic risk parameters and liquidation allocation parameters
    protocol_config.authority = ctx.accounts.authority.key();
    protocol_config.collateral_ratio = collateral_ratio;
    protocol_config.liquidation_threshold = liquidation_threshold;

    // Set fixed protocol configuration
    protocol_config.sol_price_feed = "99B2bTijsU6f1GCT73HmdR7HCFFjGMBcPZY6jZ96ynrR".parse().unwrap(); // Chainlink SOL/USD devnet
    protocol_config.usdc_mint = "USDCoctVLVnvTXBEuP9s8hntucdJokbo17RwHuNXemT".parse().unwrap(); // devnet USDC

    // Set protocol configuration parameters - liquidation allocation parameters
    protocol_config.admin_fee_rate = 1000;      
    protocol_config.liquidator_reward_rate = 300; 
    protocol_config.slippage_tolerance = 300;   
    protocol_config.admin_fee_vault = admin_fee_vault;
    protocol_config.liquidation_executor = liquidation_executor;
    protocol_config.total_admin_fees = 0;
    
    // Initialize protocol statistics
    protocol_config.total_borrowed = 0;         // Total amount borrowed from traditional pool
    protocol_config.total_deposited = 0;        // Total USDC deposited
    protocol_config.total_collateral_value = 0; // Total LP collateral value
    protocol_config.total_users = 0;            // Total number of users
    
    // Protocol status
    protocol_config.is_paused = false;
    protocol_config.created_at = Clock::get().unwrap().unix_timestamp;
    protocol_config.bump = ctx.bumps.protocol_config;
    
    // Emit protocol initialization event
    emit!(ProtocolInitialized {
        authority: protocol_config.authority,
        collateral_ratio,
        liquidation_threshold,
        sol_price_feed: protocol_config.sol_price_feed,
        raydium_pool: Pubkey::default(), // Dynamic input, no default value
        usdc_mint: protocol_config.usdc_mint,
        lp_mint: Pubkey::default(), // Dynamic input, no default value
        created_at: protocol_config.created_at,
    });

    Ok(())
}

/// Pause/Resume protocol operation status
/// Can be used in emergency situations to pause all protocol operations
#[derive(Accounts)]
pub struct ToggleProtocol<'info> {
    /// Only protocol admin can pause/resume protocol
    #[account(
        constraint = authority.key() == protocol_config.authority @ LendingError::InvalidAuthority
    )]
    pub authority: Signer<'info>,
    
    /// Protocol configuration account
    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
}

pub fn toggle_protocol_handler(ctx: Context<ToggleProtocol>) -> Result<()> {
    let protocol_config = &mut ctx.accounts.protocol_config;
    protocol_config.is_paused = !protocol_config.is_paused;
    
    let status = if protocol_config.is_paused { "Paused" } else { "Resumed" };
    
    emit!(ProtocolStatusChanged {
        authority: ctx.accounts.authority.key(),
        is_paused: protocol_config.is_paused,
    });
    
    
    Ok(())
}

/// Update protocol risk parameters
/// Admin can adjust risk parameters based on market conditions
#[derive(Accounts)]
pub struct UpdateRiskParameters<'info> {
    /// Only protocol admin can update parameters
    #[account(
        constraint = authority.key() == protocol_config.authority @ LendingError::InvalidAuthority
    )]
    pub authority: Signer<'info>,
    
    /// Protocol configuration account
    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
}

pub fn update_risk_parameters_handler(
    ctx: Context<UpdateRiskParameters>,
    new_collateral_ratio: Option<u16>,
    new_liquidation_threshold: Option<u16>,
) -> Result<()> {
    let protocol_config = &mut ctx.accounts.protocol_config;
    
    // Update collateral ratio if provided
    if let Some(collateral_ratio) = new_collateral_ratio {
        require!(
            collateral_ratio > 0 && collateral_ratio <= BASIS_POINTS as u16,
            LendingError::InvalidRiskParameters
        );
        protocol_config.collateral_ratio = collateral_ratio;
    }
    
    // Update liquidation threshold if provided
    if let Some(liquidation_threshold) = new_liquidation_threshold {
        require!(
            liquidation_threshold > BASIS_POINTS as u16,
            LendingError::InvalidRiskParameters
        );
        protocol_config.liquidation_threshold = liquidation_threshold;
    }
    
    
    emit!(RiskParametersUpdated {
        authority: ctx.accounts.authority.key(),
        collateral_ratio: protocol_config.collateral_ratio,
        liquidation_threshold: protocol_config.liquidation_threshold,
    });
    
    Ok(())
}

/// Event definitions for frontend listening and on-chain analytics

#[event]
pub struct ProtocolInitialized {
    pub authority: Pubkey,
    pub collateral_ratio: u16,
    pub liquidation_threshold: u16,
    pub sol_price_feed: Pubkey,
    pub raydium_pool: Pubkey,
    pub usdc_mint: Pubkey,
    pub lp_mint: Pubkey,
    pub created_at: i64,
}

#[event]
pub struct ProtocolStatusChanged {
    pub authority: Pubkey,
    pub is_paused: bool,
}

#[event]
pub struct RiskParametersUpdated {
    pub authority: Pubkey,
    pub collateral_ratio: u16,
    pub liquidation_threshold: u16,
}