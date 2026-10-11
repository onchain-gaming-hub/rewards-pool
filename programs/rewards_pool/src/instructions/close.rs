//! close_receipt · close_sol_asset / close_token_asset · close_pool.
//! Permissionless; rent always goes back to the recorded rent_payer.
use crate::{constants::*, errors::RewardsError, events::PoolClosed, logic, state::*};
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Burn, CloseAccount, Mint, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct CloseReceipt<'info> {
    #[account(mut)]
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mut, has_one = match_pool @ RewardsError::ReceiptAssetMismatch, has_one = rent_payer, close = rent_payer)]
    pub boost_receipt: Box<Account<'info, BoostReceipt>>,
    /// CHECK: = receipt.rent_payer.
    #[account(mut)]
    pub rent_payer: UncheckedAccount<'info>,
}

/// The boost went to rank 1 → the receipt has nothing left to prove.
pub fn handle_close_receipt(ctx: Context<CloseReceipt>) -> Result<()> {
    let pool = &mut ctx.accounts.match_pool;
    require!(pool.status == PoolStatus::Settled, RewardsError::PoolNotSettled);
    require!(logic::rank_one(pool).is_some(), RewardsError::NoRankOneWinner);
    pool.open_receipts = pool.open_receipts.checked_sub(1).ok_or(RewardsError::MathOverflow)?;
    Ok(())
}

#[derive(Accounts)]
pub struct CloseSolAsset<'info> {
    #[account(mut)]
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(
        mut,
        has_one = match_pool,
        has_one = rent_payer,
        close = rent_payer,
        constraint = pool_asset.kind == AssetKind::Sol @ RewardsError::AssetKindMismatch,
    )]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    /// CHECK: = pool_asset.rent_payer — gets the rent and any stray lamports.
    #[account(mut)]
    pub rent_payer: UncheckedAccount<'info>,
}

pub fn handle_close_sol_asset(ctx: Context<CloseSolAsset>) -> Result<()> {
    require!(logic::asset_done(&ctx.accounts.match_pool, &ctx.accounts.pool_asset)?, RewardsError::AssetNotSettled);
    let pool = &mut ctx.accounts.match_pool;
    pool.open_assets = pool.open_assets.checked_sub(1).ok_or(RewardsError::MathOverflow)?;
    Ok(())
}

#[derive(Accounts)]
pub struct CloseTokenAsset<'info> {
    #[account(mut)]
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(
        mut,
        has_one = match_pool,
        has_one = rent_payer,
        close = rent_payer,
        constraint = pool_asset.kind != AssetKind::Sol @ RewardsError::AssetKindMismatch,
    )]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    /// mut: burning strays reduces supply.
    #[account(mut, address = pool_asset.asset_id, mint::token_program = token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [VAULT_SEED, pool_asset.key().as_ref()], bump = pool_asset.vault_bump)]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: = pool_asset.rent_payer — gets the rent of the vault and the PoolAsset.
    #[account(mut)]
    pub rent_payer: UncheckedAccount<'info>,
    #[account(address = pool_asset.token_program @ RewardsError::TokenProgramMismatch)]
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_close_token_asset(ctx: Context<CloseTokenAsset>) -> Result<()> {
    let a = &ctx.accounts;
    require!(logic::asset_done(&a.match_pool, &a.pool_asset)?, RewardsError::AssetNotSettled);
    let pool_key = a.match_pool.key();
    let asset_id = a.pool_asset.asset_id;
    let seeds: &[&[u8]] = &[POOL_ASSET_SEED, pool_key.as_ref(), asset_id.as_ref(), &[a.pool_asset.bump]];
    // Everything owed is settled → whatever is left is stray (sent in directly): burn it.
    let stray = a.vault.amount;
    if stray > 0 {
        token_interface::burn(
            CpiContext::new_with_signer(
                a.token_program.key(),
                Burn { mint: a.mint.to_account_info(), from: a.vault.to_account_info(), authority: a.pool_asset.to_account_info() },
                &[seeds],
            ),
            stray,
        )?;
    }
    token_interface::close_account(CpiContext::new_with_signer(
        a.token_program.key(),
        CloseAccount {
            account: a.vault.to_account_info(),
            destination: a.rent_payer.to_account_info(),
            authority: a.pool_asset.to_account_info(),
        },
        &[seeds],
    ))?;
    let pool = &mut ctx.accounts.match_pool;
    pool.open_assets = pool.open_assets.checked_sub(1).ok_or(RewardsError::MathOverflow)?;
    Ok(())
}

#[derive(Accounts)]
pub struct ClosePool<'info> {
    #[account(mut, has_one = rent_payer, close = rent_payer)]
    pub match_pool: Box<Account<'info, MatchPool>>,
    /// CHECK: = match_pool.rent_payer (game_authority).
    #[account(mut)]
    pub rent_payer: UncheckedAccount<'info>,
}

pub fn handle_close_pool(ctx: Context<ClosePool>) -> Result<()> {
    let pool = &ctx.accounts.match_pool;
    require!(matches!(pool.status, PoolStatus::Settled | PoolStatus::Cancelled), RewardsError::InvalidStatus);
    require!(pool.open_assets == 0 && pool.open_receipts == 0, RewardsError::PoolNotEmpty);
    emit!(PoolClosed { match_pool: pool.key(), rent_payer: pool.rent_payer });
    Ok(())
}
