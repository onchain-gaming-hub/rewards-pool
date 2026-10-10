//! TEST ONLY — never deploy.
//!
//! A stand-in game program that talks to rewards-pool exactly like a real game would
//! (spec-v2 §7): only through `declare_program!` + `idls/rewards_pool.json`, signing
//! with its own `[b"game_authority"]` PDA. This is the pattern handed to maintainer.
#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

declare_id!("Davenrsu1GXQFaGtJJb65rZtwGYJH4m9x74vv8XyBMVP");

declare_program!(rewards_pool);
use rewards_pool::types::{CancelReason, CpiOutcome, NoOpReason};

pub const GAME_AUTHORITY_SEED: &[u8] = b"game_authority";

#[program]
pub mod mock_game {
    use super::*;

    /// Opens the match's pool. `game_authority` (System-owned PDA holding SOL) signs AND pays rent.
    pub fn start_match(ctx: Context<StartMatch>, room_id: u64, payout_amounts: Vec<u64>) -> Result<()> {
        let bump = ctx.bumps.game_authority;
        let seeds: &[&[u8]] = &[GAME_AUTHORITY_SEED, &[bump]];
        rewards_pool::cpi::open_pool(
            CpiContext::new_with_signer(
                rewards_pool::ID,
                rewards_pool::cpi::accounts::OpenPool {
                    game_authority: ctx.accounts.game_authority.to_account_info(),
                    game_program: ctx.accounts.this_program.to_account_info(),
                    match_pool: ctx.accounts.match_pool.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                },
                &[seeds],
            ),
            room_id,
            payout_amounts,
        )
    }

    /// Settles and reads `CpiOutcome` back. Returns it as a code (u8) so tests can see
    /// what the game saw: 0 Done · 1x NoOp(reason) · 2x Cancelled(reason).
    pub fn finish_match(ctx: Context<FinishMatch>, room_id: u64, winners: Vec<Option<Pubkey>>) -> Result<u8> {
        let bump = ctx.bumps.game_authority;
        let seeds: &[&[u8]] = &[GAME_AUTHORITY_SEED, &[bump]];
        let ret = rewards_pool::cpi::settle(
            CpiContext::new_with_signer(
                rewards_pool::ID,
                rewards_pool::cpi::accounts::Settle {
                    game_authority: ctx.accounts.game_authority.to_account_info(),
                    game_program: ctx.accounts.this_program.to_account_info(),
                    match_pool: ctx.accounts.match_pool.to_account_info(),
                },
                &[seeds],
            ),
            room_id,
            winners,
        )?;
        // Types from declare_program! have no PartialEq → `match`.
        let code = match ret.get() {
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
        };
        msg!("game saw outcome code {}", code);
        Ok(code)
    }
}

#[derive(Accounts)]
pub struct StartMatch<'info> {
    #[account(mut, seeds = [GAME_AUTHORITY_SEED], bump)]
    pub game_authority: SystemAccount<'info>,
    /// CHECK: this program's id (seed of the pool).
    #[account(address = crate::ID)]
    pub this_program: UncheckedAccount<'info>,
    /// CHECK: created by rewards-pool.
    #[account(mut)]
    pub match_pool: UncheckedAccount<'info>,
    /// CHECK: rewards-pool program (must be in the tx).
    #[account(address = rewards_pool::ID)]
    pub rewards_pool_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct FinishMatch<'info> {
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
