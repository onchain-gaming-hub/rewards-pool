//! rewards-pool — per-match prize pools for blockchain-games-hub (specs/spec-v2.md).
//!
//! Games CPI `open_pool*` / `lock_boosts` / `settle` / `cancel` signed by their
//! `[b"game_authority"]` PDA (R1). Anyone boosts; everything boosted goes to rank 1
//! after the snapshotted platform fee (R9 · D6). Claims are permissionless with pinned
//! destinations and never expire (R12).
#![allow(unexpected_cfgs)]
#![allow(clippy::result_large_err)]

use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod logic;
pub mod state;

pub use constants::*;
pub use instructions::*;
pub use state::*;

declare_id!("HgApfwjAg1L2FJL3fX7A4WX5yfhrVV8k9Ahii5Pn8HVW");

// Read-only consumer of platform-core (03): accounts only, never CPI.
declare_program!(platform_core);

#[program]
pub mod rewards_pool {
    use super::*;

    // ── game (CPI, game_authority signs) ──
    pub fn open_pool(ctx: Context<OpenPool>, room_id: u64) -> Result<()> {
        instructions::open::handle_open_pool(ctx, room_id)
    }
    pub fn open_pool_sol(ctx: Context<OpenPoolSol>, room_id: u64, payout_amounts: Vec<u64>) -> Result<()> {
        instructions::open::handle_open_pool_sol(ctx, room_id, payout_amounts)
    }
    pub fn open_pool_token(ctx: Context<OpenPoolToken>, room_id: u64, payout_amounts: Vec<u64>) -> Result<()> {
        instructions::open::handle_open_pool_token(ctx, room_id, payout_amounts)
    }
    pub fn lock_boosts(ctx: Context<GameCall>, room_id: u64) -> Result<CpiOutcome> {
        instructions::game::handle_lock_boosts(ctx, room_id)
    }
    pub fn settle(ctx: Context<GameCall>, room_id: u64, winners: Vec<Option<Pubkey>>) -> Result<CpiOutcome> {
        instructions::game::handle_settle(ctx, room_id, winners)
    }
    pub fn cancel(ctx: Context<GameCall>, room_id: u64) -> Result<CpiOutcome> {
        instructions::game::handle_cancel(ctx, room_id)
    }

    // ── boosters (own wallet — W21) ──
    pub fn boost_sol(ctx: Context<BoostSol>, amount: u64, message: String) -> Result<()> {
        instructions::boost::handle_boost_sol(ctx, amount, message)
    }
    pub fn boost_token(ctx: Context<BoostToken>, amount: u64, message: String) -> Result<()> {
        instructions::boost::handle_boost_token(ctx, amount, message)
    }

    // ── anyone ──
    pub fn expire_pool(ctx: Context<ExpirePool>) -> Result<()> {
        instructions::game::handle_expire_pool(ctx)
    }
    pub fn collect_fee_sol(ctx: Context<CollectFeeSol>) -> Result<()> {
        instructions::payout::handle_collect_fee_sol(ctx)
    }
    pub fn collect_fee_token(ctx: Context<CollectFeeToken>) -> Result<()> {
        instructions::payout::handle_collect_fee_token(ctx)
    }
    pub fn claim_sol(ctx: Context<ClaimSol>, index: u8) -> Result<()> {
        instructions::payout::handle_claim_sol(ctx, index)
    }
    pub fn claim_token(ctx: Context<ClaimToken>, index: u8) -> Result<()> {
        instructions::payout::handle_claim_token(ctx, index)
    }
    pub fn claim_token_with_ata(ctx: Context<ClaimTokenWithAta>, index: u8) -> Result<()> {
        instructions::payout::handle_claim_token_with_ata(ctx, index)
    }
    pub fn return_base_sol(ctx: Context<ReturnBaseSol>) -> Result<()> {
        instructions::payout::handle_return_base_sol(ctx)
    }
    pub fn return_base_token(ctx: Context<ReturnBaseToken>) -> Result<()> {
        instructions::payout::handle_return_base_token(ctx)
    }
    pub fn refund_boost_sol(ctx: Context<RefundBoostSol>) -> Result<()> {
        instructions::payout::handle_refund_boost_sol(ctx)
    }
    pub fn refund_boost_token(ctx: Context<RefundBoostToken>) -> Result<()> {
        instructions::payout::handle_refund_boost_token(ctx)
    }
    pub fn refund_boost_token_with_ata(ctx: Context<RefundBoostTokenWithAta>) -> Result<()> {
        instructions::payout::handle_refund_boost_token_with_ata(ctx)
    }
    pub fn close_receipt(ctx: Context<CloseReceipt>) -> Result<()> {
        instructions::close::handle_close_receipt(ctx)
    }
    pub fn close_sol_asset(ctx: Context<CloseSolAsset>) -> Result<()> {
        instructions::close::handle_close_sol_asset(ctx)
    }
    pub fn close_token_asset(ctx: Context<CloseTokenAsset>) -> Result<()> {
        instructions::close::handle_close_token_asset(ctx)
    }
    pub fn close_pool(ctx: Context<ClosePool>) -> Result<()> {
        instructions::close::handle_close_pool(ctx)
    }
}
