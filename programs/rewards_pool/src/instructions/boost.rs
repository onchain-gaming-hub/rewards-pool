//! §5.2 boost_sol / boost_token — anyone, own wallet (W21). All boosts go to rank 1 (R9).
use crate::{constants::*, errors::RewardsError, events::BoostAdded, logic, platform_core, state::*};
use anchor_lang::{prelude::*, system_program};
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};
use platform_core::accounts::AllowedMint;

/// Shared bookkeeping once the deposit is known. "Created" = version == 0 (Anchor
/// has no flag for init_if_needed); existing accounts are never re-initialised.
#[allow(clippy::too_many_arguments)]
fn record(
    pool: &mut MatchPool,
    pool_key: Pubkey,
    asset: &mut PoolAsset,
    asset_bump: u8,
    vault_bump: u8,
    receipt: &mut BoostReceipt,
    receipt_bump: u8,
    booster: Pubkey,
    kind: AssetKind,
    asset_id: Pubkey,
    token_program: Pubkey,
    decimals: u8,
    received: u64,
) -> Result<()> {
    if asset.version == 0 {
        require!(pool.boost_asset_count < MAX_BOOST_ASSETS, RewardsError::TooManyAssets);
        pool.boost_asset_count += 1;
        pool.open_assets = pool.open_assets.checked_add(1).ok_or(RewardsError::MathOverflow)?;
        asset.rent_payer = booster;
        asset.match_pool = pool_key;
        asset.version = ACCOUNT_VERSION;
        asset.kind = kind;
        asset.asset_id = asset_id;
        asset.token_program = token_program;
        asset.decimals = decimals;
        asset.bump = asset_bump;
        asset.vault_bump = vault_bump;
    } else {
        require!(asset.kind == kind, RewardsError::AssetKindMismatch);
    }
    if receipt.version == 0 {
        pool.open_receipts = pool.open_receipts.checked_add(1).ok_or(RewardsError::MathOverflow)?;
        receipt.rent_payer = booster;
        receipt.match_pool = pool_key;
        receipt.version = ACCOUNT_VERSION;
        receipt.booster = booster;
        receipt.asset_id = asset_id;
        receipt.bump = receipt_bump;
    }
    asset.boost_received = asset.boost_received.checked_add(received).ok_or(RewardsError::MathOverflow)?;
    receipt.amount = receipt.amount.checked_add(received).ok_or(RewardsError::MathOverflow)?;
    Ok(())
}

// ─────────────── SOL ───────────────

#[derive(Accounts)]
pub struct BoostSol<'info> {
    #[account(mut)]
    pub booster: Signer<'info>,
    #[account(mut, constraint = match_pool.status == PoolStatus::Open @ RewardsError::PoolNotOpen)]
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(
        init_if_needed,
        payer = booster,
        space = 8 + PoolAsset::INIT_SPACE,
        seeds = [POOL_ASSET_SEED, match_pool.key().as_ref(), Pubkey::default().as_ref()],
        bump,
    )]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    #[account(
        init_if_needed,
        payer = booster,
        space = 8 + BoostReceipt::INIT_SPACE,
        seeds = [BOOST_RECEIPT_SEED, match_pool.key().as_ref(), booster.key().as_ref(), Pubkey::default().as_ref()],
        bump,
    )]
    pub boost_receipt: Box<Account<'info, BoostReceipt>>,
    pub system_program: Program<'info, System>,
}

pub fn handle_boost_sol(ctx: Context<BoostSol>, amount: u64, message: String) -> Result<()> {
    require!(message.len() <= MAX_MESSAGE_BYTES, RewardsError::MessageTooLong);
    require!(amount >= MIN_SOL_BOOST, RewardsError::BoostTooSmall);
    // Net of fee must stay receivable by an empty wallet (fee_bps can be up to 20% — C2).
    let fee = logic::fee_on(amount, ctx.accounts.match_pool.fee_bps)?;
    require!(amount - fee >= logic::min_rent()?, RewardsError::BoostNetBelowRentExempt);

    // Amount the program moves itself (never a lamport delta: init_if_needed rent would count).
    system_program::transfer(
        CpiContext::new(
            system_program::ID,
            system_program::Transfer {
                from: ctx.accounts.booster.to_account_info(),
                to: ctx.accounts.pool_asset.to_account_info(),
            },
        ),
        amount,
    )?;
    let booster = ctx.accounts.booster.key();
    let pool_key = ctx.accounts.match_pool.key();
    record(
        &mut ctx.accounts.match_pool,
        pool_key,
        &mut ctx.accounts.pool_asset,
        ctx.bumps.pool_asset,
        0,
        &mut ctx.accounts.boost_receipt,
        ctx.bumps.boost_receipt,
        booster,
        AssetKind::Sol,
        Pubkey::default(),
        Pubkey::default(),
        9,
        amount,
    )?;
    emit!(BoostAdded { match_pool: pool_key, booster, asset_id: Pubkey::default(), amount, message });
    Ok(())
}

// ─────────────── Token / NFT ───────────────

#[derive(Accounts)]
pub struct BoostToken<'info> {
    #[account(mut)]
    pub booster: Signer<'info>,
    #[account(mut, constraint = match_pool.status == PoolStatus::Open @ RewardsError::PoolNotOpen)]
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mint::token_program = token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(
        seeds = [PC_BOOST_MINT_SEED, mint.key().as_ref()],
        bump = allowed_mint.bump,
        seeds::program = platform_core::ID,
        constraint = allowed_mint.token_program == token_program.key() @ RewardsError::TokenProgramMismatch,
    )]
    pub allowed_mint: Box<Account<'info, AllowedMint>>,
    #[account(mut, token::mint = mint, token::authority = booster, token::token_program = token_program)]
    pub booster_token: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = booster,
        space = 8 + PoolAsset::INIT_SPACE,
        seeds = [POOL_ASSET_SEED, match_pool.key().as_ref(), mint.key().as_ref()],
        bump,
    )]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    #[account(
        init_if_needed,
        payer = booster,
        seeds = [VAULT_SEED, pool_asset.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = pool_asset,
        token::token_program = token_program,
    )]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init_if_needed,
        payer = booster,
        space = 8 + BoostReceipt::INIT_SPACE,
        seeds = [BOOST_RECEIPT_SEED, match_pool.key().as_ref(), booster.key().as_ref(), mint.key().as_ref()],
        bump,
    )]
    pub boost_receipt: Box<Account<'info, BoostReceipt>>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

pub fn handle_boost_token(ctx: Context<BoostToken>, amount: u64, message: String) -> Result<()> {
    require!(message.len() <= MAX_MESSAGE_BYTES, RewardsError::MessageTooLong);
    let mint_key = ctx.accounts.mint.key();
    // Kind is frozen when the PoolAsset is created; later boosts reuse it (supply can move).
    let kind = if ctx.accounts.pool_asset.version == 0 {
        logic::classify(&ctx.accounts.mint, &mint_key)
    } else {
        ctx.accounts.pool_asset.kind
    };
    if kind == AssetKind::Nft {
        require!(amount == 1, RewardsError::InvalidNftAmount);
    }
    let decimals = ctx.accounts.mint.decimals;
    let before = ctx.accounts.vault.amount;
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.booster_token.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.booster.to_account_info(),
            },
        ),
        amount,
        decimals,
    )?;
    ctx.accounts.vault.reload()?;
    // Record what actually arrived, not the argument.
    let received = ctx.accounts.vault.amount.checked_sub(before).ok_or(RewardsError::MathOverflow)?;
    require!(received > 0, RewardsError::ZeroDeposit);

    let booster = ctx.accounts.booster.key();
    let pool_key = ctx.accounts.match_pool.key();
    let tp = ctx.accounts.token_program.key();
    if ctx.accounts.pool_asset.version != 0 {
        require_keys_eq!(ctx.accounts.pool_asset.token_program, tp, RewardsError::TokenProgramMismatch);
    }
    record(
        &mut ctx.accounts.match_pool,
        pool_key,
        &mut ctx.accounts.pool_asset,
        ctx.bumps.pool_asset,
        ctx.bumps.vault,
        &mut ctx.accounts.boost_receipt,
        ctx.bumps.boost_receipt,
        booster,
        kind,
        mint_key,
        tp,
        decimals,
        received,
    )?;
    emit!(BoostAdded { match_pool: pool_key, booster, asset_id: mint_key, amount: received, message });
    Ok(())
}
