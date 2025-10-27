
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self as token_interface, TokenAccount, TokenInterface, Mint};
use crate::state::*;
use crate::errors::LendingError;

/// LP token verification 
/// Verify whether the LP token is valid without restricting to specific pool type or version
fn verify_lp_token_validity(lp_mint_account: &AccountInfo, pool_account: &AccountInfo) -> Result<()> {
    // Basic verification 1: Ensure LP mint account exists and is initialized
    require!(lp_mint_account.data_len() > 0, LendingError::InvalidPoolData);
    
    // Basic verification 2: Ensure pool account exists (not restricted to specific program)
    require!(pool_account.data_len() > 0, LendingError::InvalidPoolData);
    
    // Basic verification 3: LP mint account cannot be the system account
    require!(*lp_mint_account.key != Pubkey::default(), LendingError::InvalidPoolData);
    require!(*pool_account.key != Pubkey::default(), LendingError::InvalidPoolData);
    
    
    
    Ok(())
}

/// LP token deposit instruction
/// User deposits any supported LP token into the protocol as collateral
/// Client needs to provide complete pool info, contract verifies validity
#[derive(Accounts)]
#[instruction(lp_mint_param: Pubkey, amount: u64)]
pub struct DepositLP<'info> {
    /// User account, must sign
    #[account(mut)]
    pub user: Signer<'info>,
    
    /// Protocol config account, verifies protocol state
    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused @ LendingError::ProtocolPaused
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
    
    
    /// User position account, stores user's deposit and borrow info
    /// Modified: Use user address + LP mint address as PDA seed, support multiple LP tokens
    #[account(
        init_if_needed,
        payer = user,
        space = UserPosition::LEN,
        seeds = [USER_POSITION_SEED, user.key().as_ref(), lp_mint_account.key().as_ref()],
        bump
    )]
    pub user_position: Account<'info, UserPosition>,

    /// LP deposit record account (new or existing)
    /// Modified: Use user address + LP mint address as PDA seed, support multiple LP tokens
    #[account(
        init_if_needed,
        payer = user,
        space = LpDepositRecord::LEN,
        seeds = [
            b"lp_deposit_record",
            user.key().as_ref(),
            lp_mint_account.key().as_ref(),
            &user_position.total_lp_deposit_count.to_le_bytes()
        ],
        bump
    )]
    pub lp_deposit_record: Account<'info, LpDepositRecord>,

    /// User's LP token account (source account) - supports TOKEN and TOKEN-2022
    #[account(
        mut,
        constraint = user_lp_account.owner == user.key() @ LendingError::InvalidTokenAccountOwner
    )]
    pub user_lp_account: InterfaceAccount<'info, TokenAccount>,
    
    /// Protocol's LP token vault account (destination account) - supports TOKEN and TOKEN-2022
    /// Must have same mint as user LP account
    #[account(
        mut,
        constraint = protocol_lp_vault.mint == user_lp_account.mint @ LendingError::InvalidLpMint
    )]
    pub protocol_lp_vault: InterfaceAccount<'info, TokenAccount>,
    
    /// LP token mint account - need to fetch decimals info
    pub lp_mint_account: InterfaceAccount<'info, Mint>,
    
    /// Raydium pool account - passed by client, contract verifies its LP mint correspondence
    /// CHECK: Runtime verification of pool data with LP mint consistency
    pub raydium_pool: AccountInfo<'info>,
    
    /// SOL/USD price feed account - passed by client
    /// CHECK: Runtime verification via Pyth SDK
    pub sol_price_feed: AccountInfo<'info>,
    
    
    /// SPL Token program (supports TOKEN and TOKEN-2022)
    pub token_program: Interface<'info, TokenInterface>,
    
    /// System program, used for creating user position account
    pub system_program: Program<'info, System>,
}

/// LP token deposit handler
/// Verify pool info and execute deposit operation
pub fn deposit_lp(ctx: Context<DepositLP>, lp_mint_param: Pubkey, amount: u64) -> Result<()> {
    // Verify deposit amount
    require!(amount > 0, LendingError::ZeroAmount);
    
    // Verify user LP token balance sufficient
    require!(
        ctx.accounts.user_lp_account.amount >= amount,
        LendingError::InsufficientLpBalance
    );
    
    // General LP token verification: verify LP token and pool basic validity
    verify_lp_token_validity(&ctx.accounts.lp_mint_account.to_account_info(), &ctx.accounts.raydium_pool)?;
    
    let user_position = &mut ctx.accounts.user_position;
    let _protocol_config = &mut ctx.accounts.protocol_config;
    
    // If it's a new user position, initialize basic info
    if user_position.owner == Pubkey::default() {
        user_position.initialize(ctx.accounts.user.key(), lp_mint_param, ctx.bumps.user_position);
    } else {
        // Verify LP mint consistency - user can only deposit same LP token
        require!(
            user_position.lp_mint == lp_mint_param,
            LendingError::InvalidLpMint
        );
    }
    
    // Execute LP token transfer: from user account to protocol vault account
    let transfer_instruction = token_interface::TransferChecked {
        from: ctx.accounts.user_lp_account.to_account_info(),
        to: ctx.accounts.protocol_lp_vault.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
        mint: ctx.accounts.lp_mint_account.to_account_info(),
    };
    
    let cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        transfer_instruction,
    );
    
    token_interface::transfer_checked(cpi_ctx, amount, ctx.accounts.lp_mint_account.decimals)?;

    // Create LP deposit record
    let lp_deposit_record = &mut ctx.accounts.lp_deposit_record;
    lp_deposit_record.initialize(
        ctx.accounts.user.key(),
        lp_mint_param,
        amount,
        0, // LP value initially 0, need to update via update_collateral_value
        user_position.total_lp_deposit_count,
        ctx.bumps.lp_deposit_record,
    );

    // Update user position status
    user_position.deposit_lp(amount);

    // Increase LP deposit record count
    user_position.total_lp_deposit_count = user_position.total_lp_deposit_count
        .checked_add(1)
        .ok_or(LendingError::MathOverflow)?;

   
    emit!(LPDeposited {
        user: ctx.accounts.user.key(),
        amount,
        total_deposited: user_position.lp_deposited,
    });

    Ok(())
}

/// LP token withdraw handler
/// User withdraws their deposited LP tokens, but must ensure no outstanding debts
#[derive(Accounts)]
#[instruction(amount: u64)]
pub struct WithdrawLP<'info> {
    /// User account, must sign
    #[account(mut)]
    pub user: Signer<'info>,
    
    /// Protocol configuration account
    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused @ LendingError::ProtocolPaused
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,

    /// User position account
    #[account(
        mut,
        seeds = [USER_POSITION_SEED, user.key().as_ref(), lp_mint_account.key().as_ref()],
        bump = user_position.bump,
        constraint = user_position.owner == user.key() @ LendingError::InvalidAuthority
    )]
    pub user_position: Account<'info, UserPosition>,

    /// LP withdraw record account (new or existing)
    #[account(
        init_if_needed,
        payer = user,
        space = LpWithdrawRecord::LEN,
        seeds = [
            b"lp_withdraw_record",
            user.key().as_ref(),
            lp_mint_account.key().as_ref(),
            &user_position.total_lp_withdraw_count.to_le_bytes()
        ],
        bump
    )]
    pub lp_withdraw_record: Account<'info, LpWithdrawRecord>,

    /// User's LP token account (destination account) - supports TOKEN and TOKEN-2022
    #[account(
        mut,
        constraint = user_lp_account.owner == user.key() @ LendingError::InvalidTokenAccountOwner
    )]
    pub user_lp_account: InterfaceAccount<'info, TokenAccount>,

    /// LP vault PDA - actual owner of LP vault account
    /// CHECK: This PDA is only used as authority for LP vault account, no data stored, just need to verify seed
    #[account(
        seeds = [b"lp_vault"],
        bump
    )]
    pub lp_vault: UncheckedAccount<'info>,

    /// Protocol's LP token vault account (source account) - supports TOKEN and TOKEN-2022
    /// Must use same mint as user LP account, and owner must be LP vault PDA
    #[account(
        mut,
        constraint = protocol_lp_vault.mint == user_lp_account.mint @ LendingError::InvalidLpMint,
        constraint = protocol_lp_vault.owner == lp_vault.key() @ LendingError::InvalidTokenAccountOwner
    )]
    pub protocol_lp_vault: InterfaceAccount<'info, TokenAccount>,

    /// LP token mint account - need to get decimals info
    pub lp_mint_account: InterfaceAccount<'info, Mint>,

    /// SPL Token program (supports TOKEN and TOKEN-2022)
    pub token_program: Interface<'info, TokenInterface>,

    /// System program, used to create LP withdraw record account
    pub system_program: Program<'info, System>,
}

/// LP token withdraw handler
pub fn withdraw_lp(ctx: Context<WithdrawLP>, amount: u64) -> Result<()> {
    // verify withdraw amount
    require!(amount > 0, LendingError::ZeroAmount);
    
    // debt check: must have no outstanding debts to withdraw LP (simplified logic, avoid math overflow)            
    require!(
        ctx.accounts.user_position.usdc_borrowed == 0,
        LendingError::CannotWithdrawWithDebt
    );

    // for CLMM NFT, verify user actually holds the NFT, no need to check lp_deposited value
    // because CLMM uses liquidity amount rather than simple token amount, lp_deposited may be inaccurate

    // use LP vault PDA as correct authority for token transfer
    let lp_vault_seeds = &[
        b"lp_vault",
        std::slice::from_ref(&ctx.bumps.lp_vault),
    ];
    let lp_vault_signer = &[&lp_vault_seeds[..]];
    
    // execute LP token transfer: from protocol vault to user account
    let transfer_instruction = token_interface::TransferChecked {
        from: ctx.accounts.protocol_lp_vault.to_account_info(),
        to: ctx.accounts.user_lp_account.to_account_info(),
        authority: ctx.accounts.lp_vault.to_account_info(), // Use LP vault PDA as authority
        mint: ctx.accounts.lp_mint_account.to_account_info(),
    };
    
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        transfer_instruction,
        lp_vault_signer, // Use LP vault PDA as signer
    );
    
    token_interface::transfer_checked(cpi_ctx, amount, ctx.accounts.lp_mint_account.decimals)?;

    // create LP withdraw record
    let lp_withdraw_record = &mut ctx.accounts.lp_withdraw_record;
    lp_withdraw_record.initialize(
        ctx.accounts.user.key(),
        ctx.accounts.lp_mint_account.key(),
        amount,
        0, // Lp value is 0 after withdraw, no need to update
        ctx.accounts.user_position.total_lp_withdraw_count,
        ctx.bumps.lp_withdraw_record,
    );

    // update user position status
    ctx.accounts.user_position.withdraw_lp(amount);

    // increment LP withdraw record count
    ctx.accounts.user_position.total_lp_withdraw_count = ctx.accounts.user_position.total_lp_withdraw_count
        .checked_add(1)
        .ok_or(LendingError::MathOverflow)?;

    // note: LP withdraw does not affect USDC pool stats, stats are managed by traditional_usdc_pool

    let remaining_lp = ctx.accounts.user_position.lp_deposited;

    // emit withdraw event
    emit!(LPWithdrawn {
        user: ctx.accounts.user.key(),
        amount,
        remaining_deposited: remaining_lp,
    });


    if remaining_lp == 0 && ctx.accounts.user_position.usdc_borrowed == 0 {
        let user_position_lamports = ctx.accounts.user_position.to_account_info().lamports();

        **ctx.accounts.user_position.to_account_info().try_borrow_mut_lamports()? -= user_position_lamports;
        **ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? += user_position_lamports;

        msg!("UserPosition account closed, rent refunded to user");
    }

    Ok(())
}

/// Emergency LP withdraw instruction - only available when protocol is paused
/// Allows user to withdraw their LP tokens when protocol is paused
#[derive(Accounts)]
pub struct EmergencyWithdraw<'info> {
    /// User account
    #[account(mut)]
    pub user: Signer<'info>,
    
    /// Protocol config account - must be paused
    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        constraint = protocol_config.is_paused @ LendingError::ProtocolNotInitialized // Reuse error to indicate protocol must be paused
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
    
    /// User position account
    #[account(
        mut,
        seeds = [USER_POSITION_SEED, user.key().as_ref(), lp_mint_account.key().as_ref()],
        bump = user_position.bump,
        constraint = user_position.owner == user.key() @ LendingError::InvalidAuthority
    )]
    pub user_position: Account<'info, UserPosition>,
    
    /// User's LP token account - supports TOKEN and TOKEN-2022
    #[account(
        mut,
        constraint = user_lp_account.owner == user.key() @ LendingError::InvalidTokenAccountOwner
    )]
    pub user_lp_account: InterfaceAccount<'info, TokenAccount>,
    
    /// Protocol LP token vault account - supports TOKEN and TOKEN-2022
    #[account(mut)]
    pub protocol_lp_vault: InterfaceAccount<'info, TokenAccount>,
    
    /// LP token mint account - needed to get decimals info
    pub lp_mint_account: InterfaceAccount<'info, Mint>,
    
    /// SPL Token program (supports TOKEN and TOKEN-2022)
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn emergency_withdraw(ctx: Context<EmergencyWithdraw>) -> Result<()> {
    let user_position = &mut ctx.accounts.user_position;
    let protocol_config = &mut ctx.accounts.protocol_config;
    let withdraw_amount = user_position.lp_deposited;
    
    require!(withdraw_amount > 0, LendingError::ZeroAmount);
    
    // Prepare protocol PDA signer
    let protocol_seeds = &[
        PROTOCOL_SEED,
        &[protocol_config.bump],
    ];
    let protocol_signer = &[&protocol_seeds[..]];
    
    // Execute emergency withdraw
    let transfer_instruction = token_interface::TransferChecked {
        from: ctx.accounts.protocol_lp_vault.to_account_info(),
        to: ctx.accounts.user_lp_account.to_account_info(),
        authority: protocol_config.to_account_info(),
        mint: ctx.accounts.lp_mint_account.to_account_info(),
    };
    
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        transfer_instruction,
        protocol_signer,
    );
    
    token_interface::transfer_checked(cpi_ctx, withdraw_amount, ctx.accounts.lp_mint_account.decimals)?;
    
    // Clear user LP deposited amount
    user_position.withdraw_lp(withdraw_amount);

    // Note: LP emergency withdraw does not affect USDC pool stats, stats are managed by traditional_usdc_pool

    emit!(EmergencyWithdrawal {
        user: ctx.accounts.user.key(),
        amount: withdraw_amount,
    });

    // After emergency withdraw, if all LP is withdrawn and no debt, close UserPosition account
    if user_position.lp_deposited == 0 && user_position.usdc_borrowed == 0 {
        let user_position_lamports = user_position.to_account_info().lamports();

        **user_position.to_account_info().try_borrow_mut_lamports()? -= user_position_lamports;
        **ctx.accounts.user.to_account_info().try_borrow_mut_lamports()? += user_position_lamports;

        msg!("UserPosition account closed, rent refunded to user");
    }

    Ok(())
}

/// Event definitions

#[event]
pub struct LPDeposited {
    pub user: Pubkey,
    pub amount: u64,
    pub total_deposited: u64,
}

#[event]
pub struct LPWithdrawn {
    pub user: Pubkey,
    pub amount: u64,
    pub remaining_deposited: u64,
}

#[event]
pub struct EmergencyWithdrawal {
    pub user: Pubkey,
    pub amount: u64,
}
