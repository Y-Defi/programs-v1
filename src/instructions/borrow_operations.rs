use anchor_lang::prelude::*;
use anchor_spl::token::{self as token, Transfer, Token};
use anchor_spl::token::TokenAccount;
use crate::state::*;
use crate::errors::LendingError;
use crate::utils::{calculate_health_factor, get_sol_price};
use crate::instructions::raydium_integration::get_raydium_pool_data;
use crate::instructions::valuation::{parse_personal_position_data, parse_clmm_current_tick, calculate_token_amounts_from_liquidity, calculate_value_from_token_amounts};
// use crate::{safe_add, safe_sub};

/// USDC Borrow Instruction
/// User borrows USDC based on LP token deposit - traditional bank model
#[derive(Accounts)]
pub struct BorrowUSDC<'info> {
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
    
    /// User position account, stores user's deposit and borrow information
    /// Modification: Requires specifying which LP token to use as collateral
    #[account(
        mut,
        constraint = user_position.owner == user.key() @ LendingError::InvalidAuthority,
        constraint = user_position.lp_deposited > 0 @ LendingError::InsufficientDepositBalance
    )]
    pub user_position: Account<'info, UserPosition>,
    
    /// Traditional USDC pool account
    #[account(
        mut,
        seeds = [b"traditional_usdc_pool", protocol_config.usdc_mint.as_ref()],
        bump,
        constraint = traditional_usdc_pool.usdc_mint == protocol_config.usdc_mint @ LendingError::InvalidUsdcMint
    )]
    pub traditional_usdc_pool: Account<'info, TraditionalUsdcPool>,
    
    /// User borrow record account (new or existing)
    #[account(
        init_if_needed,
        payer = user,
        space = UserBorrowRecord::LEN,
        seeds = [
            b"user_borrow_record",
            user.key().as_ref(),
            user_position.lp_mint.as_ref(),
            &user_position.total_borrow_count.to_le_bytes()
        ],
        bump
    )]
    pub borrow_record: Account<'info, UserBorrowRecord>,
    
    /// User's USDC token account (destination account)
    #[account(
        mut,
        constraint = user_usdc_account.mint == protocol_config.usdc_mint @ LendingError::InvalidUsdcMint,
        constraint = user_usdc_account.owner == user.key() @ LendingError::InvalidTokenAccountOwner
    )]
    pub user_usdc_account: Account<'info, TokenAccount>,
    
    /// Traditional USDC pool vault account (source account)
    #[account(
        mut,
        seeds = [b"traditional_usdc_vault", traditional_usdc_pool.key().as_ref()],
        bump
    )]
    pub pool_usdc_vault: Account<'info, TokenAccount>,
    
    /// Chainlink SOL/USD price feed account
    /// CHECK: use Chainlink SDK to verify price data validity
    pub chainlink_sol_feed: UncheckedAccount<'info>,
    
    /// Chainlink program account
    /// CHECK: use Chainlink SDK to verify price data validity
    pub chainlink_program: UncheckedAccount<'info>,
    
    /// Raydium pool account, used to get reserve information
    /// CHECK: use CPI to call Raydium program to verify data validity
    pub raydium_pool: UncheckedAccount<'info>,
    
    /// User's NFT position account (only needed for CLMM pool)
    /// CHECK: verify this is the correct position account for user's LP mint
    pub user_nft_position: Option<UncheckedAccount<'info>>,
    
    /// SPL Token program
    pub token_program: Program<'info, Token>,
    /// System program
    pub system_program: Program<'info, System>,
}

/// USDC borrow handler - traditional bank model with LP token as collateral
pub fn borrow_usdc(ctx: Context<BorrowUSDC>, _lp_mint: Pubkey, amount: u64) -> Result<()> {
    require!(amount > 0, LendingError::ZeroAmount);
    
    // Check traditional USDC pool liquidity
    require!(
        ctx.accounts.pool_usdc_vault.amount >= amount,
        LendingError::InsufficientReserve
    );
    
    // Re-calculate collateral value on borrow
    let collateral_value = calculate_collateral_value(&ctx)?;
    let health_factor = if ctx.accounts.user_position.usdc_borrowed == 0 {
        u64::MAX
    } else {
        // unified precision to 6 decimal places
        let collateral_value_6_decimals = collateral_value / 100;
        calculate_health_factor(collateral_value_6_decimals, ctx.accounts.user_position.usdc_borrowed)
    };
    // Update cache with new health factor
    ctx.accounts.user_position.update_cache(collateral_value, health_factor);
    
    validate_borrow_amount(&ctx, amount, collateral_value)?;
    
    // In traditional pool, record borrow (lock current interest rate)
    let traditional_pool = &mut ctx.accounts.traditional_usdc_pool;
    traditional_pool.borrow(amount)?;
    
    // Create user borrow record
    let borrow_record = &mut ctx.accounts.borrow_record;
    borrow_record.initialize(
        ctx.accounts.user.key(),
        ctx.accounts.user_position.lp_mint, 
        amount,
        traditional_pool.current_borrow_rate, 
        ctx.accounts.user_position.total_borrow_count,
        ctx.bumps.borrow_record,
    );
    
    // Execute USDC transfer from traditional pool vault to user account
    execute_traditional_usdc_transfer(&ctx, amount)?;
    
    // Update borrow state with traditional pool data
    update_borrow_state_with_traditional_pool(ctx, amount, collateral_value)?;
    
    Ok(())
}

/// Calculate collateral value - only supports CLMM pools
fn calculate_collateral_value(ctx: &Context<BorrowUSDC>) -> Result<u64> {
    let sol_price_value = get_sol_price(&ctx.accounts.chainlink_program, &ctx.accounts.chainlink_sol_feed)?;
    let _clmm_info = get_raydium_pool_data(&ctx.accounts.raydium_pool)?;
    
    // Only supports CLMM pools - returns raw LP value, no discount applied
    // Discount should be applied when calculating borrow ability, not here
    calculate_clmm_value_optimized(ctx, sol_price_value)
}

/// Optimized CLMM value calculation - minimizes stack usage
fn calculate_clmm_value_optimized(ctx: &Context<BorrowUSDC>, sol_price_value: u64) -> Result<u64> {
    
    // Get user NFT position account
    let user_nft_position = ctx.accounts.user_nft_position.as_ref()
        .ok_or(LendingError::InvalidPoolData)?;
    
    // Parse PersonalPosition account data
    let position_data = parse_personal_position_data(&user_nft_position.data.borrow())?;

    // Get current tick - immediately release borrow
    let current_tick = {
        let pool_data = ctx.accounts.raydium_pool.try_borrow_data()?;
        let tick = parse_clmm_current_tick(&pool_data)?;
        drop(pool_data); // Immediately release borrow
        tick
    };
    
    // Calculate token amounts
    let (token0_amount, token1_amount) = calculate_token_amounts_from_liquidity(
        position_data.liquidity,
        current_tick,
        position_data.tick_lower_index,
        position_data.tick_upper_index,
    )?;
    
    // Calculate and return total USD value
    calculate_value_from_token_amounts(token0_amount, token1_amount, sol_price_value, 1_000_000)
}

/// Validate borrow amount - separate function to minimize stack usage
fn validate_borrow_amount(ctx: &Context<BorrowUSDC>, amount: u64, collateral_value: u64) -> Result<()> {
    // Calculate total debt after borrow
    let total_debt_after_borrow = ctx.accounts.user_position.usdc_borrowed
        .checked_add(amount)
        .ok_or(LendingError::MathOverflow)?;
    
    // Calculate max allowed borrow amount
    let discount_factor = 8000u16; 
    let max_total_borrow = crate::math::RiskCalculator::calculate_borrow_limit(
        collateral_value,
        discount_factor, 
        ctx.accounts.protocol_config.collateral_ratio, 
    )?;

    // Convert to 6 decimal places (USDC format)
    let max_total_borrow = max_total_borrow / 100;

    // Check if borrow amount exceeds allowed limit
    require!(total_debt_after_borrow <= max_total_borrow, LendingError::InsufficientBorrowLimit);
    Ok(())
}

/// Execute USDC transfer from traditional pool vault to user account
fn execute_traditional_usdc_transfer(ctx: &Context<BorrowUSDC>, amount: u64) -> Result<()> {
    // According to traditional pool initialization, vault authority is usdc_pool
    // Need to use usdc_pool's PDA as signer
    let usdc_mint = ctx.accounts.protocol_config.usdc_mint;
    let seeds = &[
        b"traditional_usdc_pool",
        usdc_mint.as_ref(),
        &[ctx.bumps.traditional_usdc_pool],
    ];
    let signer = &[&seeds[..]];
    
    let transfer_cpi = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        Transfer {
            from: ctx.accounts.pool_usdc_vault.to_account_info(),
            to: ctx.accounts.user_usdc_account.to_account_info(),
            authority: ctx.accounts.traditional_usdc_pool.to_account_info(), 
        },
        signer,
    );
    
    token::transfer(transfer_cpi, amount)
}

/// Update borrow state - integrates traditional pool mode
fn update_borrow_state_with_traditional_pool(ctx: Context<BorrowUSDC>, amount: u64, collateral_value: u64) -> Result<()> {
    // Note: Traditional pool state has already been updated in main function through traditional_pool.borrow()
    // No need to repeat update here to avoid double deduction of available_liquidity
    
    // Update user position's total borrow amount
    ctx.accounts.user_position.add_borrow(amount);
    
    // Increase user's borrow record count
    ctx.accounts.user_position.total_borrow_count = ctx.accounts.user_position.total_borrow_count
        .checked_add(1)
        .ok_or(LendingError::MathOverflow)?;
    
    // Recalculate health factor - using 6 decimal precision
    let collateral_value_6_decimals = collateral_value / 100; // 转换为6位小数
    let health_factor = calculate_health_factor(collateral_value_6_decimals, ctx.accounts.user_position.usdc_borrowed);
    ctx.accounts.user_position.update_cache(collateral_value, health_factor);
    
    // Update protocol total borrow amount statistic
    ctx.accounts.protocol_config.total_borrowed = ctx.accounts.protocol_config.total_borrowed
        .checked_add(amount)
        .ok_or(LendingError::MathOverflow)?;
    
    // Update protocol total collateral value statistic
    ctx.accounts.protocol_config.total_collateral_value = collateral_value;
    
    emit!(USDCBorrowed {
        user: ctx.accounts.user.key(),
        amount,
        total_borrowed: ctx.accounts.user_position.usdc_borrowed,
        collateral_value_usd: ctx.accounts.user_position.collateral_value_cache,
        health_factor: ctx.accounts.user_position.health_factor_cache,
    });

    Ok(())
}

/// USDC Repayment Instruction - integrates traditional bank mode
/// Users repay USDC they've borrowed, with interest calculated at the time of borrowing
#[derive(Accounts)]
pub struct RepayUSDC<'info> {
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

    /// Traditional USDC pool account
    #[account(
        mut,
        constraint = traditional_usdc_pool.usdc_mint == protocol_config.usdc_mint @ LendingError::InvalidUsdcMint
    )]
    pub traditional_usdc_pool: Account<'info, TraditionalUsdcPool>,

    /// User borrow record account
    #[account(
        mut,
        constraint = borrow_record.owner == user.key() @ LendingError::InvalidAuthority,
        constraint = borrow_record.pool == user_position.lp_mint @ LendingError::InvalidLpMint,
        constraint = !borrow_record.is_repaid @ LendingError::UserPositionNotFound
    )]
    pub borrow_record: Account<'info, UserBorrowRecord>,

    /// User position account
    #[account(
        mut,
        constraint = user_position.owner == user.key() @ LendingError::InvalidAuthority,
        constraint = user_position.usdc_borrowed > 0 @ LendingError::UserPositionNotFound
    )]
    pub user_position: Account<'info, UserPosition>,

    /// User repay record account (new or existing)
    #[account(
        init_if_needed,
        payer = user,
        space = UserRepayRecord::LEN,
        seeds = [
            b"user_repay_record",
            user.key().as_ref(),
            user_position.lp_mint.as_ref(),  
            &user_position.total_repay_count.to_le_bytes()
        ],
        bump
    )]
    pub repay_record: Account<'info, UserRepayRecord>,

    /// User's USDC token account (source account)
    #[account(
        mut,
        constraint = user_usdc_account.mint == protocol_config.usdc_mint @ LendingError::InvalidUsdcMint,
        constraint = user_usdc_account.owner == user.key() @ LendingError::InvalidTokenAccountOwner
    )]
    pub user_usdc_account: Account<'info, TokenAccount>,

    /// Traditional USDC pool vault account (target account)
    #[account(
        mut,
        seeds = [b"traditional_usdc_vault", traditional_usdc_pool.key().as_ref()],
        bump
    )]
    pub pool_usdc_vault: Account<'info, TokenAccount>,

    /// SPL Token program
    pub token_program: Program<'info, Token>,

    /// System program
    pub system_program: Program<'info, System>,
}

/// USDC repayment handler - calculates interest at the time of borrowing
pub fn repay_usdc(ctx: Context<RepayUSDC>, amount: u64) -> Result<()> {
    require!(amount > 0, LendingError::ZeroAmount);
    
    let borrow_record = &mut ctx.accounts.borrow_record;
    let traditional_pool = &mut ctx.accounts.traditional_usdc_pool;
    let user_position = &mut ctx.accounts.user_position;
    let protocol_config = &mut ctx.accounts.protocol_config;
    
    // Calculate interest at the time of borrowing
    // Calculate time elapsed in minutes
    let current_timestamp = Clock::get()?.unix_timestamp;
    let time_elapsed = current_timestamp - borrow_record.borrow_timestamp;

    // Formula: Interest = Remaining Principal × Borrow Rate × Time Elapsed (minutes) / (365 × 24 × 60 × 10000)
    let interest = TraditionalUsdcPool::calculate_borrow_interest(
        borrow_record.remaining_principal,
        borrow_record.locked_borrow_rate, 
        borrow_record.borrow_timestamp,
    )?;

    let total_debt = borrow_record.remaining_principal
        .checked_add(interest)
        .ok_or(LendingError::ArithmeticOverflow)?;
    
    // Determine actual repayment amount
    let actual_repay_amount = if amount >= total_debt {
        total_debt // Repay all remaining principal and interest
    } else {
        require!(amount >= 1_000_000, LendingError::ZeroAmount); // Partial repay requires at least 1 USDC
        amount
    };

    // Verify user has sufficient USDC balance
    require!(
        ctx.accounts.user_usdc_account.amount >= actual_repay_amount,
        LendingError::InsufficientDebt
    );


    let (principal_repaid, interest_repaid) = if actual_repay_amount >= total_debt {
        // Full repayment: repay all remaining principal and interest
        (borrow_record.remaining_principal, interest)
    } else {
        // Partial repayment: pay interest first, then principal
        let interest_to_pay = interest.min(actual_repay_amount);
        let principal_to_pay = actual_repay_amount.saturating_sub(interest_to_pay);
        (principal_to_pay, interest_to_pay)
    };

    // Execute USDC transfer: from user account to traditional pool vault
    let transfer_instruction = Transfer {
        from: ctx.accounts.user_usdc_account.to_account_info(),
        to: ctx.accounts.pool_usdc_vault.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
    };

    let cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        transfer_instruction,
    );

    token::transfer(cpi_ctx, actual_repay_amount)?;

    // Update borrow record status
    borrow_record.process_repay(principal_repaid, interest_repaid);

    // Update user position's total borrowed amount
    user_position.repay_borrow(principal_repaid);

    // Update traditional pool's state (only call once)
    traditional_pool.repay(principal_repaid, interest_repaid)?;

    // Recalculate health factor
    if user_position.usdc_borrowed > 0 {
        let current_collateral_value = user_position.collateral_value_cache;
        let health_factor = crate::utils::calculate_health_factor(
            current_collateral_value,
            user_position.usdc_borrowed,
        );
        user_position.update_cache(current_collateral_value, health_factor);
    } else {
        // Set health factor to max value when all debt is repaid
        let collateral_cache = user_position.collateral_value_cache;
        user_position.update_cache(collateral_cache, u64::MAX);
    }
    
    // Update protocol total borrowed amount
    protocol_config.total_borrowed = protocol_config.total_borrowed
        .checked_sub(principal_repaid)
        .ok_or(LendingError::MathOverflow)?;

    // Create repay record
    let repay_record = &mut ctx.accounts.repay_record;
    repay_record.initialize(
        ctx.accounts.user.key(),
        user_position.lp_mint,  // Use lp_mint instead of traditional_pool.key()
        traditional_pool.key(),
        principal_repaid,
        interest_repaid,
        user_position.total_repay_count,
        borrow_record.borrow_index, // Link to borrow record index
        ctx.bumps.repay_record,
    );

    // Increment repay record count
    user_position.total_repay_count = user_position.total_repay_count
        .checked_add(1)
        .ok_or(LendingError::MathOverflow)?;

    // Emit repay event
    emit!(USDCRepaid {
        user: ctx.accounts.user.key(),
        amount: actual_repay_amount,
        remaining_debt: user_position.usdc_borrowed,
        health_factor: user_position.health_factor_cache,
    });


    Ok(())
}

/// Batch repay instruction - repay all debt at once (using traditional pool)
/// Simplify user operations by repaying all debt in one transaction``
#[derive(Accounts)]
pub struct RepayAll<'info> {
    /// User account
    #[account(mut)]
    pub user: Signer<'info>,
    
    /// Protocol config account
    #[account(
        mut,
        seeds = [PROTOCOL_SEED],
        bump = protocol_config.bump,
        constraint = !protocol_config.is_paused @ LendingError::ProtocolPaused
    )]
    pub protocol_config: Account<'info, ProtocolConfig>,
    
    /// Traditional USDC pool account
    #[account(
        mut,
        constraint = traditional_usdc_pool.usdc_mint == protocol_config.usdc_mint @ LendingError::InvalidUsdcMint
    )]
    pub traditional_usdc_pool: Account<'info, TraditionalUsdcPool>,
    
    /// User borrow record account (need to find via seeds, simplified here)
    #[account(
        mut,
        constraint = borrow_record.owner == user.key() @ LendingError::InvalidAuthority,
        constraint = borrow_record.pool == user_position.lp_mint @ LendingError::InvalidLpMint,
        constraint = !borrow_record.is_repaid @ LendingError::UserPositionNotFound
    )]
    pub borrow_record: Account<'info, UserBorrowRecord>,
    
    /// User position account
    #[account(
        mut,
        constraint = user_position.owner == user.key() @ LendingError::InvalidAuthority,
        constraint = user_position.usdc_borrowed > 0 @ LendingError::UserPositionNotFound
    )]
    pub user_position: Account<'info, UserPosition>,
    
    /// User USDC account
    #[account(
        mut,
        constraint = user_usdc_account.mint == protocol_config.usdc_mint @ LendingError::InvalidUsdcMint,
        constraint = user_usdc_account.owner == user.key() @ LendingError::InvalidTokenAccountOwner
    )]
    pub user_usdc_account: Account<'info, TokenAccount>,
    
    /// Traditional USDC pool vault account
    #[account(
        mut,
        seeds = [b"traditional_usdc_vault", traditional_usdc_pool.key().as_ref()],
        bump
    )]
    pub pool_usdc_vault: Account<'info, TokenAccount>,
    
    /// SPL Token program account
    pub token_program: Program<'info, Token>,
}

/// Repay all debt handler - repay all debt at once (using traditional pool)
/// Simplify user operations by repaying all debt in one transaction
pub fn repay_all(ctx: Context<RepayAll>) -> Result<()> {
    let borrow_record = &mut ctx.accounts.borrow_record;
    let traditional_pool = &mut ctx.accounts.traditional_usdc_pool;
    let user_position = &mut ctx.accounts.user_position;
    let protocol_config = &mut ctx.accounts.protocol_config;
    
    // Calculate total interest based on locked borrow rate at borrow time
    let current_timestamp = Clock::get()?.unix_timestamp;
    let time_elapsed = current_timestamp - borrow_record.borrow_timestamp;
    
    let interest = TraditionalUsdcPool::calculate_borrow_interest(
        borrow_record.remaining_principal,
        borrow_record.locked_borrow_rate, // Use borrow time locked rate
        borrow_record.borrow_timestamp,
    )?;

    let total_debt = borrow_record.remaining_principal
        .checked_add(interest)
        .ok_or(LendingError::ArithmeticOverflow)?;

    require!(total_debt > 0, LendingError::UserPositionNotFound);

    // Verify user USDC balance is sufficient to repay all debt (principal + interest)
    require!(
        ctx.accounts.user_usdc_account.amount >= total_debt,
        LendingError::InsufficientProtocolReserves
    );

    // Update traditional pool state
    traditional_pool.repay(borrow_record.remaining_principal, interest)?;
    
    // Execute full repayment transfer (principal + interest)
    let transfer_instruction = Transfer {
        from: ctx.accounts.user_usdc_account.to_account_info(),
        to: ctx.accounts.pool_usdc_vault.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
    };
    
    let cpi_ctx = CpiContext::new(
        ctx.accounts.token_program.to_account_info(),
        transfer_instruction,
    );
    
    token::transfer(cpi_ctx, total_debt)?;
    
    // Mark borrow record as repaid
    let remaining_principal = borrow_record.remaining_principal;
    borrow_record.process_repay(remaining_principal, interest);

    // Clear user debt
    user_position.usdc_borrowed = 0;
    user_position.health_factor_cache = u64::MAX; // Health factor max when no debt
    user_position.liquidation_pending = false;
    user_position.last_update_slot = Clock::get()?.slot;

    // Update protocol total borrowed stats
    protocol_config.total_borrowed = protocol_config.total_borrowed
        .checked_sub(remaining_principal)
        .ok_or(LendingError::MathOverflow)?;
    
    emit!(AllDebtRepaid {
        user: ctx.accounts.user.key(),
        amount: total_debt,
    });
    
    
    Ok(())
}


/// Event definitions for frontend listening and on-chain analytics

#[event]
pub struct USDCBorrowed {
    pub user: Pubkey,
    pub amount: u64,
    pub total_borrowed: u64,
    pub collateral_value_usd: u64,
    pub health_factor: u64,
}

#[event]
pub struct USDCRepaid {
    pub user: Pubkey,
    pub amount: u64,
    pub remaining_debt: u64,
    pub health_factor: u64,
}

#[event]
pub struct AllDebtRepaid {
    pub user: Pubkey,
    pub amount: u64,
}

