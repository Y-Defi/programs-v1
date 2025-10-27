// Traditional USDC Pool Instructions

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Mint, Transfer};
use crate::state::*;
use crate::errors::LendingError;

// ============ Initialization Instructions ============

#[derive(Accounts)]
pub struct InitializeTraditionalUsdcPool<'info> {
    #[account(
        init,
        payer = authority,
        space = TraditionalUsdcPool::LEN,
        seeds = [b"traditional_usdc_pool", usdc_mint.key().as_ref()],
        bump
    )]
    pub usdc_pool: Account<'info, TraditionalUsdcPool>,

    #[account(
        init,
        payer = authority,
        token::mint = usdc_mint,
        token::authority = usdc_pool,
        seeds = [b"traditional_usdc_vault", usdc_pool.key().as_ref()],
        bump
    )]
    pub pool_usdc_vault: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, Mint>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

/// Initialize Traditional USDC Pool
pub fn initialize_traditional_usdc_pool(
    ctx: Context<InitializeTraditionalUsdcPool>,
    initial_deposit_rate: u16,  // e.g., 500 = 5%
    initial_borrow_rate: u16,   // e.g., 700 = 7%
    min_deposit_amount: u64,    // e.g., 1_000_000 = 1 USDC
) -> Result<()> {
    let pool = &mut ctx.accounts.usdc_pool;
    let bump = ctx.bumps.usdc_pool;
    
    pool.initialize(
        ctx.accounts.authority.key(),
        ctx.accounts.usdc_mint.key(),
        ctx.accounts.pool_usdc_vault.key(),
        initial_deposit_rate,
        initial_borrow_rate,
        min_deposit_amount,
        bump,
    );

    
    Ok(())
}

// ============ Deposit Instructions ============

#[derive(Accounts)]
#[instruction(amount: u64, deposit_index: u32)]
pub struct TraditionalDeposit<'info> {
    #[account(mut)]
    pub usdc_pool: Account<'info, TraditionalUsdcPool>,

    #[account(
        init,
        payer = depositor,
        space = UserDepositRecord::LEN,
        seeds = [
            b"user_deposit_record",
            depositor.key().as_ref(),
            usdc_pool.key().as_ref(),
            &deposit_index.to_le_bytes()
        ],
        bump
    )]
    pub deposit_record: Account<'info, UserDepositRecord>,

    #[account(mut)]
    pub pool_usdc_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = depositor_usdc_account.mint == usdc_pool.usdc_mint,
        constraint = depositor_usdc_account.owner == depositor.key()
    )]
    pub depositor_usdc_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub depositor: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// Traditional USDC Pool Deposit
pub fn traditional_deposit(
    ctx: Context<TraditionalDeposit>,
    amount: u64,
    deposit_index: u32,
) -> Result<()> {
    // Get immutable references first
    let pool_key = ctx.accounts.usdc_pool.key();
    let depositor_key = ctx.accounts.depositor.key();
    let bump = ctx.bumps.deposit_record;

    // Process pool state
    {
        let pool = &mut ctx.accounts.usdc_pool;
        pool.deposit(amount)?;
    }

    // Record deposit details
    let deposit_record = &mut ctx.accounts.deposit_record;
    deposit_record.initialize(
        depositor_key,
        pool_key,
        amount,
        deposit_index,
        bump,
    );

    // Transfer USDC to pool vault
    let transfer_cpi = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.depositor_usdc_account.to_account_info(),
            to: ctx.accounts.pool_usdc_vault.to_account_info(),
            authority: ctx.accounts.depositor.to_account_info(),
        },
    );
    token::transfer(transfer_cpi, amount)?;


    Ok(())
}

// ============ Withdraw Instructions ============

#[derive(Accounts)]
pub struct TraditionalWithdraw<'info> {
    #[account(mut)]
    pub usdc_pool: Account<'info, TraditionalUsdcPool>,

    #[account(
        mut,
        constraint = deposit_record.owner == withdrawer.key(),
        constraint = deposit_record.pool == usdc_pool.key(),
        constraint = !deposit_record.is_withdrawn @ LendingError::UserDepositNotFound
    )]
    pub deposit_record: Account<'info, UserDepositRecord>,

    /// User withdraw record account (new)
    #[account(
        init,
        payer = withdrawer,
        space = UserWithdrawRecord::LEN,
        seeds = [
            b"user_withdraw_record",
            withdrawer.key().as_ref(),
            usdc_pool.key().as_ref(),
            &deposit_record.deposit_index.to_le_bytes()
        ],
        bump
    )]
    pub withdraw_record: Account<'info, UserWithdrawRecord>,

    #[account(
        mut,
        seeds = [b"traditional_usdc_vault", usdc_pool.key().as_ref()],
        bump
    )]
    pub pool_usdc_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = withdrawer_usdc_account.mint == usdc_pool.usdc_mint,
        constraint = withdrawer_usdc_account.owner == withdrawer.key()
    )]
    pub withdrawer_usdc_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub withdrawer: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

/// Traditional USDC Pool Withdraw
pub fn traditional_withdraw(ctx: Context<TraditionalWithdraw>) -> Result<()> {
    let pool = &mut ctx.accounts.usdc_pool;
    let deposit_record = &mut ctx.accounts.deposit_record;

    // Calculate interest (using current rate at withdraw time)
    let interest = pool.calculate_deposit_interest(
        deposit_record.principal_amount,
        deposit_record.deposit_timestamp,
    )?;

    let total_amount = deposit_record.principal_amount
        .checked_add(interest)
        .ok_or(LendingError::ArithmeticOverflow)?;

    // Update pool state
    pool.withdraw(deposit_record.principal_amount, interest)?;

    // Mark deposit record as withdrawn
    deposit_record.mark_withdrawn(interest);

    // Create withdraw record
    let withdraw_record = &mut ctx.accounts.withdraw_record;
    withdraw_record.initialize(
        ctx.accounts.withdrawer.key(),
        pool.key(),
        deposit_record.principal_amount,
        interest,
        deposit_record.deposit_index, // Use associated deposit index as withdraw index
        deposit_record.deposit_index, // Associated deposit index
        ctx.bumps.withdraw_record,
    );

    // Generate PDA signer for transfer
    let pool_key = pool.key();
    let vault_bump = ctx.bumps.pool_usdc_vault;
    let seeds = &[
        b"traditional_usdc_vault",
        pool_key.as_ref(),
        &[vault_bump],
    ];
    let _signer_seeds = &[&seeds[..]];

    // Transfer principal + interest to user - pool as authority, using pool's PDA signer
    let pool_seeds = &[
        b"traditional_usdc_pool",
        ctx.accounts.usdc_pool.usdc_mint.as_ref(),
        &[ctx.accounts.usdc_pool.bump],
    ];
    let pool_signer_seeds = &[&pool_seeds[..]];

    let transfer_cpi = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.pool_usdc_vault.to_account_info(),
            to: ctx.accounts.withdrawer_usdc_account.to_account_info(),
            authority: ctx.accounts.usdc_pool.to_account_info(), // Use pool as authority
        },
        pool_signer_seeds,
    );
    token::transfer(transfer_cpi, total_amount)?;


    Ok(())
}

// Borrow/Repay functionality is implemented in borrow_operations.rs
// Traditional USDC Pool only manages liquidity, not directly handles borrowing logic

// ============ Interest Rate Update Instructions ============

#[derive(Accounts)]
pub struct UpdateTraditionalRates<'info> {
    #[account(
        mut,
        constraint = usdc_pool.authority == authority.key() @ LendingError::Unauthorized
    )]
    pub usdc_pool: Account<'info, TraditionalUsdcPool>,

    pub authority: Signer<'info>,
}

/// Update deposit and borrow rates (only admin)
pub fn update_traditional_rates(
    ctx: Context<UpdateTraditionalRates>,
    new_deposit_rate: Option<u16>,
    new_borrow_rate: Option<u16>,
) -> Result<()> {
    let pool = &mut ctx.accounts.usdc_pool;
    
    pool.update_rates(new_deposit_rate, new_borrow_rate)?;

    if let Some(rate) = new_deposit_rate {
    }
    
    if let Some(rate) = new_borrow_rate {
    }


    Ok(())
}