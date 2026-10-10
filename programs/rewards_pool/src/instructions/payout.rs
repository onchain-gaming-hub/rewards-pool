//! §5.6 collect_fee · §5.8–5.9 claim · §5.10 return_base · §5.11 refund_boost.
//! All permissionless; every destination is pinned (§5.7). No Active / AllowedMint
//! checks on any of these paths (03 §1 rule 3).
use super::util::{lamports_out, vault_transfer};
use crate::{
    constants::*,
    errors::RewardsError,
    events::{BaseReturned, BoostRefunded, Claimed, FeeCollected},
    logic, platform_core,
    state::*,
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use platform_core::accounts::PlatformConfig;

// ═════════════════════════ collect_fee ═════════════════════════

fn fee_due(pool: &MatchPool, asset: &PoolAsset) -> Result<u64> {
    require!(pool.status == PoolStatus::Settled, RewardsError::PoolNotSettled);
    require!(logic::rank_one(pool).is_some(), RewardsError::NoRankOneWinner);
    require!(asset.kind != AssetKind::Nft, RewardsError::NoFeeForNft);
    require!(!asset.fee_collected, RewardsError::FeeAlreadyCollected);
    logic::fee(pool, asset)
}

#[derive(Accounts)]
pub struct CollectFeeSol<'info> {
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mut, has_one = match_pool, constraint = pool_asset.kind == AssetKind::Sol @ RewardsError::AssetKindMismatch)]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    #[account(seeds = [PC_CONFIG_SEED], bump = config.bump, seeds::program = platform_core::ID)]
    pub config: Box<Account<'info, PlatformConfig>>,
    /// CHECK: pinned by seeds + seeds::program (03 §2.6).
    #[account(mut, seeds = [PC_TREASURY_SEED], bump = config.treasury_bump, seeds::program = platform_core::ID)]
    pub treasury: UncheckedAccount<'info>,
}

pub fn handle_collect_fee_sol(ctx: Context<CollectFeeSol>) -> Result<()> {
    let fee = fee_due(&ctx.accounts.match_pool, &ctx.accounts.pool_asset)?;
    ctx.accounts.pool_asset.fee_collected = true; // mark before moving value (§11.7)
    if fee > 0 {
        lamports_out(&ctx.accounts.pool_asset.to_account_info(), &ctx.accounts.treasury.to_account_info(), fee)?;
    }
    emit!(FeeCollected { match_pool: ctx.accounts.match_pool.key(), asset_id: Pubkey::default(), fee });
    Ok(())
}

#[derive(Accounts)]
pub struct CollectFeeToken<'info> {
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mut, has_one = match_pool, constraint = pool_asset.kind != AssetKind::Sol @ RewardsError::AssetKindMismatch)]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    #[account(address = pool_asset.asset_id, mint::token_program = token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [VAULT_SEED, pool_asset.key().as_ref()], bump = pool_asset.vault_bump)]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(seeds = [PC_CONFIG_SEED], bump = config.bump, seeds::program = platform_core::ID)]
    pub config: Box<Account<'info, PlatformConfig>>,
    /// CHECK: pinned by seeds + seeds::program — authority of the Treasury ATA.
    #[account(seeds = [PC_TREASURY_SEED], bump = config.treasury_bump, seeds::program = platform_core::ID)]
    pub treasury: UncheckedAccount<'info>,
    /// Exists already (platform-core creates it at allow_mint — X3). Never init here.
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = treasury,
        associated_token::token_program = token_program,
    )]
    pub treasury_ata: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address = pool_asset.token_program @ RewardsError::TokenProgramMismatch)]
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_collect_fee_token(ctx: Context<CollectFeeToken>) -> Result<()> {
    let fee = fee_due(&ctx.accounts.match_pool, &ctx.accounts.pool_asset)?;
    ctx.accounts.pool_asset.fee_collected = true; // mark before moving value (§11.7)
    let a = &ctx.accounts;
    if fee > 0 {
        vault_transfer(
            a.token_program.to_account_info(),
            a.vault.to_account_info(),
            a.mint.to_account_info(),
            a.treasury_ata.to_account_info(),
            a.pool_asset.to_account_info(),
            a.match_pool.key(),
            a.pool_asset.asset_id,
            a.pool_asset.bump,
            fee,
            a.pool_asset.decimals,
        )?;
    }
    let asset_id = ctx.accounts.pool_asset.asset_id;
    emit!(FeeCollected { match_pool: ctx.accounts.match_pool.key(), asset_id, fee });
    Ok(())
}

// ═════════════════════════ claim ═════════════════════════

/// Validates and marks the claim; returns the amount to send.
fn take_claim(pool: &MatchPool, asset: &mut PoolAsset, index: u8, winner: &Pubkey) -> Result<u64> {
    require!(pool.status == PoolStatus::Settled, RewardsError::PoolNotSettled);
    let i = index as usize;
    require!(logic::winner_at(pool, i) == Some(*winner), RewardsError::NoWinnerAtIndex);
    let bit = 1u32 << i;
    require!(asset.claimed & bit == 0, RewardsError::AlreadyClaimed);
    let amount = logic::payout(pool, asset, i)?;
    require!(amount > 0, RewardsError::NothingToClaim);
    asset.claimed |= bit; // mark before moving value
    Ok(amount)
}

#[derive(Accounts)]
#[instruction(index: u8)]
pub struct ClaimSol<'info> {
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mut, has_one = match_pool, constraint = pool_asset.kind == AssetKind::Sol @ RewardsError::AssetKindMismatch)]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    /// CHECK: pinned to winners[index] in `take_claim`. UncheckedAccount (not SystemAccount)
    /// so program-owned wallets (e.g. Squads) can receive. May be marked signer by the
    /// client when sent through the relayer.
    #[account(mut)]
    pub winner: UncheckedAccount<'info>,
}

pub fn handle_claim_sol(ctx: Context<ClaimSol>, index: u8) -> Result<()> {
    let winner = ctx.accounts.winner.key();
    let amount = take_claim(&ctx.accounts.match_pool, &mut ctx.accounts.pool_asset, index, &winner)?;
    lamports_out(&ctx.accounts.pool_asset.to_account_info(), &ctx.accounts.winner.to_account_info(), amount)?;
    emit!(Claimed { match_pool: ctx.accounts.match_pool.key(), index, player: winner, asset_id: Pubkey::default(), amount });
    Ok(())
}

#[derive(Accounts)]
#[instruction(index: u8)]
pub struct ClaimToken<'info> {
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mut, has_one = match_pool, constraint = pool_asset.kind != AssetKind::Sol @ RewardsError::AssetKindMismatch)]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    #[account(address = pool_asset.asset_id, mint::token_program = token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [VAULT_SEED, pool_asset.key().as_ref()], bump = pool_asset.vault_bump)]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: pinned to winners[index] in `take_claim`.
    pub winner: UncheckedAccount<'info>,
    /// Must already exist (Q-10) — otherwise use claim_token_with_ata.
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = winner,
        associated_token::token_program = token_program,
    )]
    pub winner_ata: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address = pool_asset.token_program @ RewardsError::TokenProgramMismatch)]
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_claim_token(ctx: Context<ClaimToken>, index: u8) -> Result<()> {
    let winner = ctx.accounts.winner.key();
    let amount = take_claim(&ctx.accounts.match_pool, &mut ctx.accounts.pool_asset, index, &winner)?;
    let a = &ctx.accounts;
    vault_transfer(
        a.token_program.to_account_info(),
        a.vault.to_account_info(),
        a.mint.to_account_info(),
        a.winner_ata.to_account_info(),
        a.pool_asset.to_account_info(),
        a.match_pool.key(),
        a.pool_asset.asset_id,
        a.pool_asset.bump,
        amount,
        a.pool_asset.decimals,
    )?;
    emit!(Claimed { match_pool: a.match_pool.key(), index, player: winner, asset_id: a.pool_asset.asset_id, amount });
    Ok(())
}

#[derive(Accounts)]
#[instruction(index: u8)]
pub struct ClaimTokenWithAta<'info> {
    /// Pays the ATA rent — usually the player (F3), may be anyone (crank / platform).
    #[account(mut)]
    pub payer: Signer<'info>,
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mut, has_one = match_pool, constraint = pool_asset.kind != AssetKind::Sol @ RewardsError::AssetKindMismatch)]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    #[account(address = pool_asset.asset_id, mint::token_program = token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [VAULT_SEED, pool_asset.key().as_ref()], bump = pool_asset.vault_bump)]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,
    /// CHECK: pinned to winners[index] in `take_claim`.
    pub winner: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = winner,
        associated_token::token_program = token_program,
    )]
    pub winner_ata: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address = pool_asset.token_program @ RewardsError::TokenProgramMismatch)]
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_claim_token_with_ata(ctx: Context<ClaimTokenWithAta>, index: u8) -> Result<()> {
    let winner = ctx.accounts.winner.key();
    let amount = take_claim(&ctx.accounts.match_pool, &mut ctx.accounts.pool_asset, index, &winner)?;
    let a = &ctx.accounts;
    vault_transfer(
        a.token_program.to_account_info(),
        a.vault.to_account_info(),
        a.mint.to_account_info(),
        a.winner_ata.to_account_info(),
        a.pool_asset.to_account_info(),
        a.match_pool.key(),
        a.pool_asset.asset_id,
        a.pool_asset.bump,
        amount,
        a.pool_asset.decimals,
    )?;
    emit!(Claimed { match_pool: a.match_pool.key(), index, player: winner, asset_id: a.pool_asset.asset_id, amount });
    Ok(())
}

// ═════════════════════════ return_base ═════════════════════════

fn take_base(pool: &MatchPool, asset: &mut PoolAsset) -> Result<u64> {
    require!(logic::is_base(pool, asset), RewardsError::NotBaseAsset);
    require!(matches!(pool.status, PoolStatus::Settled | PoolStatus::Cancelled), RewardsError::InvalidStatus);
    require!(!asset.base_returned, RewardsError::BaseAlreadyReturned);
    let amount = logic::unclaimed_base(pool, asset)?;
    asset.base_returned = true;
    Ok(amount)
}

#[derive(Accounts)]
pub struct ReturnBaseSol<'info> {
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mut, has_one = match_pool, constraint = pool_asset.kind == AssetKind::Sol @ RewardsError::AssetKindMismatch)]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    /// CHECK: = match_pool.base_source (game_authority for a SOL base).
    #[account(mut, address = match_pool.base_source @ RewardsError::BaseAccountsMismatch)]
    pub base_source: UncheckedAccount<'info>,
}

pub fn handle_return_base_sol(ctx: Context<ReturnBaseSol>) -> Result<()> {
    let amount = take_base(&ctx.accounts.match_pool, &mut ctx.accounts.pool_asset)?;
    if amount > 0 {
        lamports_out(&ctx.accounts.pool_asset.to_account_info(), &ctx.accounts.base_source.to_account_info(), amount)?;
    }
    emit!(BaseReturned { match_pool: ctx.accounts.match_pool.key(), asset_id: Pubkey::default(), amount, to: ctx.accounts.base_source.key() });
    Ok(())
}

#[derive(Accounts)]
pub struct ReturnBaseToken<'info> {
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mut, has_one = match_pool, constraint = pool_asset.kind != AssetKind::Sol @ RewardsError::AssetKindMismatch)]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    #[account(address = pool_asset.asset_id, mint::token_program = token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [VAULT_SEED, pool_asset.key().as_ref()], bump = pool_asset.vault_bump)]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, address = match_pool.base_source @ RewardsError::BaseAccountsMismatch)]
    pub base_source: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address = pool_asset.token_program @ RewardsError::TokenProgramMismatch)]
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_return_base_token(ctx: Context<ReturnBaseToken>) -> Result<()> {
    let amount = take_base(&ctx.accounts.match_pool, &mut ctx.accounts.pool_asset)?;
    let a = &ctx.accounts;
    if amount > 0 {
        vault_transfer(
            a.token_program.to_account_info(),
            a.vault.to_account_info(),
            a.mint.to_account_info(),
            a.base_source.to_account_info(),
            a.pool_asset.to_account_info(),
            a.match_pool.key(),
            a.pool_asset.asset_id,
            a.pool_asset.bump,
            amount,
            a.pool_asset.decimals,
        )?;
    }
    emit!(BaseReturned { match_pool: a.match_pool.key(), asset_id: a.pool_asset.asset_id, amount, to: a.base_source.key() });
    Ok(())
}

// ═════════════════════════ refund_boost ═════════════════════════

fn take_refund(pool: &mut MatchPool, asset: &mut PoolAsset, receipt: &BoostReceipt) -> Result<u64> {
    logic::require_terminal(pool)?;
    require!(logic::boost_refundable(pool), RewardsError::RefundNotAllowed);
    require!(receipt.asset_id == asset.asset_id, RewardsError::ReceiptAssetMismatch);
    asset.boost_refunded = asset.boost_refunded.checked_add(receipt.amount).ok_or(RewardsError::MathOverflow)?;
    pool.open_receipts = pool.open_receipts.checked_sub(1).ok_or(RewardsError::MathOverflow)?;
    Ok(receipt.amount)
}

#[derive(Accounts)]
pub struct RefundBoostSol<'info> {
    #[account(mut)]
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mut, has_one = match_pool, constraint = pool_asset.kind == AssetKind::Sol @ RewardsError::AssetKindMismatch)]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    #[account(mut, has_one = match_pool @ RewardsError::ReceiptAssetMismatch, has_one = booster, close = booster)] // close to booster == receipt.rent_payer (boost.rs record)
    pub boost_receipt: Box<Account<'info, BoostReceipt>>,
    /// CHECK: = receipt.booster (= receipt.rent_payer) — refund + receipt rent go here.
    #[account(mut)]
    pub booster: UncheckedAccount<'info>,
}

pub fn handle_refund_boost_sol(ctx: Context<RefundBoostSol>) -> Result<()> {
    let amount = take_refund(&mut ctx.accounts.match_pool, &mut ctx.accounts.pool_asset, &ctx.accounts.boost_receipt)?;
    lamports_out(&ctx.accounts.pool_asset.to_account_info(), &ctx.accounts.booster.to_account_info(), amount)?;
    emit!(BoostRefunded { match_pool: ctx.accounts.match_pool.key(), booster: ctx.accounts.booster.key(), asset_id: Pubkey::default(), amount });
    Ok(())
}

#[derive(Accounts)]
pub struct RefundBoostToken<'info> {
    #[account(mut)]
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mut, has_one = match_pool, constraint = pool_asset.kind != AssetKind::Sol @ RewardsError::AssetKindMismatch)]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    #[account(address = pool_asset.asset_id, mint::token_program = token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [VAULT_SEED, pool_asset.key().as_ref()], bump = pool_asset.vault_bump)]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, has_one = match_pool @ RewardsError::ReceiptAssetMismatch, has_one = booster, close = booster)] // close to booster == receipt.rent_payer (boost.rs record)
    pub boost_receipt: Box<Account<'info, BoostReceipt>>,
    /// CHECK: = receipt.booster.
    #[account(mut)]
    pub booster: UncheckedAccount<'info>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = booster,
        associated_token::token_program = token_program,
    )]
    pub booster_ata: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address = pool_asset.token_program @ RewardsError::TokenProgramMismatch)]
    pub token_program: Interface<'info, TokenInterface>,
}

pub fn handle_refund_boost_token(ctx: Context<RefundBoostToken>) -> Result<()> {
    let amount = take_refund(&mut ctx.accounts.match_pool, &mut ctx.accounts.pool_asset, &ctx.accounts.boost_receipt)?;
    let a = &ctx.accounts;
    vault_transfer(
        a.token_program.to_account_info(),
        a.vault.to_account_info(),
        a.mint.to_account_info(),
        a.booster_ata.to_account_info(),
        a.pool_asset.to_account_info(),
        a.match_pool.key(),
        a.pool_asset.asset_id,
        a.pool_asset.bump,
        amount,
        a.pool_asset.decimals,
    )?;
    emit!(BoostRefunded { match_pool: a.match_pool.key(), booster: a.booster.key(), asset_id: a.pool_asset.asset_id, amount });
    Ok(())
}

#[derive(Accounts)]
pub struct RefundBoostTokenWithAta<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mut)]
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mut, has_one = match_pool, constraint = pool_asset.kind != AssetKind::Sol @ RewardsError::AssetKindMismatch)]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    #[account(address = pool_asset.asset_id, mint::token_program = token_program)]
    pub mint: Box<InterfaceAccount<'info, Mint>>,
    #[account(mut, seeds = [VAULT_SEED, pool_asset.key().as_ref()], bump = pool_asset.vault_bump)]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(mut, has_one = match_pool @ RewardsError::ReceiptAssetMismatch, has_one = booster, close = booster)] // close to booster == receipt.rent_payer (boost.rs record)
    pub boost_receipt: Box<Account<'info, BoostReceipt>>,
    /// CHECK: = receipt.booster.
    #[account(mut)]
    pub booster: UncheckedAccount<'info>,
    #[account(
        init_if_needed,
        payer = payer,
        associated_token::mint = mint,
        associated_token::authority = booster,
        associated_token::token_program = token_program,
    )]
    pub booster_ata: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(address = pool_asset.token_program @ RewardsError::TokenProgramMismatch)]
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_refund_boost_token_with_ata(ctx: Context<RefundBoostTokenWithAta>) -> Result<()> {
    let amount = take_refund(&mut ctx.accounts.match_pool, &mut ctx.accounts.pool_asset, &ctx.accounts.boost_receipt)?;
    let a = &ctx.accounts;
    vault_transfer(
        a.token_program.to_account_info(),
        a.vault.to_account_info(),
        a.mint.to_account_info(),
        a.booster_ata.to_account_info(),
        a.pool_asset.to_account_info(),
        a.match_pool.key(),
        a.pool_asset.asset_id,
        a.pool_asset.bump,
        amount,
        a.pool_asset.decimals,
    )?;
    emit!(BoostRefunded { match_pool: a.match_pool.key(), booster: a.booster.key(), asset_id: a.pool_asset.asset_id, amount });
    Ok(())
}
