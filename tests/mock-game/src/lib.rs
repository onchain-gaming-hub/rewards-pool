//! TEST ONLY — never deploy.
//!
//! A stand-in game program that talks to rewards-pool exactly like a real game would
//! (spec-v2 §7): only through `declare_program!` + `idls/rewards_pool.json`, signing
//! with its own `[b"game_authority"]` PDA (System-owned, holds SOL for rent).
//! This is the pattern handed to maintainer.
#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

declare_id!("Davenrsu1GXQFaGtJJb65rZtwGYJH4m9x74vv8XyBMVP");

declare_program!(rewards_pool);
use rewards_pool::types::{CancelReason, CpiOutcome, NoOpReason};

pub const GAME_AUTHORITY_SEED: &[u8] = b"game_authority";

/// What the game saw, as a u8 the tests can read from return data:
/// 0 Done · 10+ NoOp(AlreadyLocked, AlreadySettled, AlreadyCancelled, Closed) ·
/// 20+ Cancelled(ByGame, NoEligibleWinner, Expired).
fn code(o: CpiOutcome) -> u8 {
    // Types from declare_program! have no PartialEq → `match`.
    match o {
        CpiOutcome::Done => 0,
        CpiOutcome::NoOp(r) => {
            10 + match r {
                NoOpReason::AlreadyLocked => 0,
                NoOpReason::AlreadySettled => 1,
                NoOpReason::AlreadyCancelled => 2,
                NoOpReason::Closed => 3,
            }
        }
        CpiOutcome::Cancelled(r) => {
            20 + match r {
                CancelReason::ByGame => 0,
                CancelReason::NoEligibleWinner => 1,
                CancelReason::Expired => 2,
            }
        }
    }
}

#[program]
pub mod mock_game {
    use super::*;

    pub fn start_match(ctx: Context<StartMatch>, room_id: u64) -> Result<()> {
        let seeds: &[&[u8]] = &[GAME_AUTHORITY_SEED, &[ctx.bumps.game_authority]];
        let a = &ctx.accounts;
        rewards_pool::cpi::open_pool(
            CpiContext::new_with_signer(
                rewards_pool::ID,
                rewards_pool::cpi::accounts::OpenPool {
                    game_authority: a.game_authority.to_account_info(),
                    game_program: a.this_program.to_account_info(),
                    game_entry: a.game_entry.to_account_info(),
                    config: a.config.to_account_info(),
                    match_pool: a.match_pool.to_account_info(),
                    system_program: a.system_program.to_account_info(),
                },
                &[seeds],
            ),
            room_id,
        )
    }

    pub fn start_match_sol(ctx: Context<StartMatchSol>, room_id: u64, payout_amounts: Vec<u64>) -> Result<()> {
        let seeds: &[&[u8]] = &[GAME_AUTHORITY_SEED, &[ctx.bumps.game_authority]];
        let a = &ctx.accounts;
        rewards_pool::cpi::open_pool_sol(
            CpiContext::new_with_signer(
                rewards_pool::ID,
                rewards_pool::cpi::accounts::OpenPoolSol {
                    game_authority: a.game_authority.to_account_info(),
                    game_program: a.this_program.to_account_info(),
                    game_entry: a.game_entry.to_account_info(),
                    config: a.config.to_account_info(),
                    match_pool: a.match_pool.to_account_info(),
                    pool_asset: a.pool_asset.to_account_info(),
                    system_program: a.system_program.to_account_info(),
                },
                &[seeds],
            ),
            room_id,
            payout_amounts,
        )
    }

    pub fn start_match_token(ctx: Context<StartMatchToken>, room_id: u64, payout_amounts: Vec<u64>) -> Result<()> {
        let seeds: &[&[u8]] = &[GAME_AUTHORITY_SEED, &[ctx.bumps.game_authority]];
        let a = &ctx.accounts;
        rewards_pool::cpi::open_pool_token(
            CpiContext::new_with_signer(
                rewards_pool::ID,
                rewards_pool::cpi::accounts::OpenPoolToken {
                    game_authority: a.game_authority.to_account_info(),
                    game_program: a.this_program.to_account_info(),
                    game_entry: a.game_entry.to_account_info(),
                    config: a.config.to_account_info(),
                    match_pool: a.match_pool.to_account_info(),
                    base_mint: a.base_mint.to_account_info(),
                    allowed_mint: a.allowed_mint.to_account_info(),
                    base_source: a.base_source.to_account_info(),
                    pool_asset: a.pool_asset.to_account_info(),
                    vault: a.vault.to_account_info(),
                    token_program: a.token_program.to_account_info(),
                    associated_token_program: a.associated_token_program.to_account_info(),
                    system_program: a.system_program.to_account_info(),
                },
                &[seeds],
            ),
            room_id,
            payout_amounts,
        )
    }

    pub fn lock(ctx: Context<GameCallAccounts>, room_id: u64) -> Result<u8> {
        let seeds: &[&[u8]] = &[GAME_AUTHORITY_SEED, &[ctx.bumps.game_authority]];
        let a = &ctx.accounts;
        let ret = rewards_pool::cpi::lock_boosts(
            CpiContext::new_with_signer(
                rewards_pool::ID,
                rewards_pool::cpi::accounts::LockBoosts {
                    game_authority: a.game_authority.to_account_info(),
                    game_program: a.this_program.to_account_info(),
                    match_pool: a.match_pool.to_account_info(),
                },
                &[seeds],
            ),
            room_id,
        )?;
        Ok(code(ret.get()))
    }

    pub fn finish_match(ctx: Context<GameCallAccounts>, room_id: u64, winners: Vec<Option<Pubkey>>) -> Result<u8> {
        let seeds: &[&[u8]] = &[GAME_AUTHORITY_SEED, &[ctx.bumps.game_authority]];
        let a = &ctx.accounts;
        let ret = rewards_pool::cpi::settle(
            CpiContext::new_with_signer(
                rewards_pool::ID,
                rewards_pool::cpi::accounts::Settle {
                    game_authority: a.game_authority.to_account_info(),
                    game_program: a.this_program.to_account_info(),
                    match_pool: a.match_pool.to_account_info(),
                },
                &[seeds],
            ),
            room_id,
            winners,
        )?;
        let c = code(ret.get());
        msg!("game saw outcome code {}", c);
        Ok(c)
    }

    pub fn cancel_match(ctx: Context<GameCallAccounts>, room_id: u64) -> Result<u8> {
        let seeds: &[&[u8]] = &[GAME_AUTHORITY_SEED, &[ctx.bumps.game_authority]];
        let a = &ctx.accounts;
        let ret = rewards_pool::cpi::cancel(
            CpiContext::new_with_signer(
                rewards_pool::ID,
                rewards_pool::cpi::accounts::Cancel {
                    game_authority: a.game_authority.to_account_info(),
                    game_program: a.this_program.to_account_info(),
                    match_pool: a.match_pool.to_account_info(),
                },
                &[seeds],
            ),
            room_id,
        )?;
        Ok(code(ret.get()))
    }
}

#[derive(Accounts)]
pub struct StartMatch<'info> {
    #[account(mut, seeds = [GAME_AUTHORITY_SEED], bump)]
    pub game_authority: SystemAccount<'info>,
    /// CHECK: this program's id.
    #[account(address = crate::ID)]
    pub this_program: UncheckedAccount<'info>,
    /// CHECK: platform-core GameEntry — rewards-pool checks it.
    pub game_entry: UncheckedAccount<'info>,
    /// CHECK: platform-core PlatformConfig — rewards-pool checks it.
    pub config: UncheckedAccount<'info>,
    /// CHECK: created by rewards-pool.
    #[account(mut)]
    pub match_pool: UncheckedAccount<'info>,
    /// CHECK: rewards-pool program (must be in the tx).
    #[account(address = rewards_pool::ID)]
    pub rewards_pool_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct StartMatchSol<'info> {
    #[account(mut, seeds = [GAME_AUTHORITY_SEED], bump)]
    pub game_authority: SystemAccount<'info>,
    /// CHECK: this program's id.
    #[account(address = crate::ID)]
    pub this_program: UncheckedAccount<'info>,
    /// CHECK: checked by rewards-pool.
    pub game_entry: UncheckedAccount<'info>,
    /// CHECK: checked by rewards-pool.
    pub config: UncheckedAccount<'info>,
    /// CHECK: created by rewards-pool.
    #[account(mut)]
    pub match_pool: UncheckedAccount<'info>,
    /// CHECK: created by rewards-pool.
    #[account(mut)]
    pub pool_asset: UncheckedAccount<'info>,
    /// CHECK: rewards-pool program.
    #[account(address = rewards_pool::ID)]
    pub rewards_pool_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct StartMatchToken<'info> {
    #[account(mut, seeds = [GAME_AUTHORITY_SEED], bump)]
    pub game_authority: SystemAccount<'info>,
    /// CHECK: this program's id.
    #[account(address = crate::ID)]
    pub this_program: UncheckedAccount<'info>,
    /// CHECK: checked by rewards-pool.
    pub game_entry: UncheckedAccount<'info>,
    /// CHECK: checked by rewards-pool.
    pub config: UncheckedAccount<'info>,
    /// CHECK: created by rewards-pool.
    #[account(mut)]
    pub match_pool: UncheckedAccount<'info>,
    /// CHECK: checked by rewards-pool.
    pub base_mint: UncheckedAccount<'info>,
    /// CHECK: checked by rewards-pool.
    pub allowed_mint: UncheckedAccount<'info>,
    /// CHECK: checked by rewards-pool (ATA of game_authority).
    #[account(mut)]
    pub base_source: UncheckedAccount<'info>,
    /// CHECK: created by rewards-pool.
    #[account(mut)]
    pub pool_asset: UncheckedAccount<'info>,
    /// CHECK: created by rewards-pool.
    #[account(mut)]
    pub vault: UncheckedAccount<'info>,
    /// CHECK: token program of the base mint.
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: associated token program.
    pub associated_token_program: UncheckedAccount<'info>,
    /// CHECK: rewards-pool program.
    #[account(address = rewards_pool::ID)]
    pub rewards_pool_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct GameCallAccounts<'info> {
    #[account(seeds = [GAME_AUTHORITY_SEED], bump)]
    pub game_authority: SystemAccount<'info>,
    /// CHECK: this program's id.
    #[account(address = crate::ID)]
    pub this_program: UncheckedAccount<'info>,
    /// CHECK: may be empty (closed pool) — rewards-pool checks it.
    #[account(mut)]
    pub match_pool: UncheckedAccount<'info>,
    /// CHECK: rewards-pool program.
    #[account(address = rewards_pool::ID)]
    pub rewards_pool_program: UncheckedAccount<'info>,
}
