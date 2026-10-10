//! rewards-pool — **POC subset** of `specs/spec-v2.md` (§13). Not the final program.
//!
//! Each instruction here exists to prove one unverified claim of the spec:
//! - `open_pool`      — §13 #1: a game's System-owned `game_authority` PDA can be the
//!                      `payer` of `init` through CPI (Q-13).
//! - `settle`         — §13 #4: `CpiOutcome` comes back to the game as an Anchor return
//!                      value through `declare_program!`; closed pool → `NoOp(Closed)`.
//! - `poc_init_vault` — §13 #3: Anchor `init` + `token::` sizes a Token-2022 vault for
//!                      the mint's required account extensions (Pausable → PausableAccount)
//!                      and survives an address that was pre-funded with lamports.
//! - `poc_classify`   — §3.4: the NFT predicate (decimals 0 ∧ supply 1 ∧ authority None
//!                      or == Token Metadata master edition PDA).
//! - `poc_dup_keys`   — §13 #5 (Anchor side): the same key in a `mut Signer` field and a
//!                      read-only `UncheckedAccount` field of one instruction.
//! - `poc_close_pool` — test helper to reach the "account closed" state.
//! Platform-core checks (GameEntry / Config / AllowedMint) are deliberately left out:
//! they are already proven by platform-core's mock-consumer.
#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;
use anchor_lang::solana_program::program_option::COption;
use anchor_spl::token_interface::{Mint, TokenAccount, TokenInterface};

declare_id!("2DmMWR2e3xVtwB5Kjp41QmNh4K8DP8uMXshxxTxwWd7q");

pub const MAX_WINNERS: usize = 32;
pub const MATCH_POOL_SEED: &[u8] = b"match_pool";
pub const GAME_AUTHORITY_SEED: &[u8] = b"game_authority";
pub const VAULT_SEED: &[u8] = b"vault";
/// Token Metadata program (`metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s`).
pub const TOKEN_METADATA_ID: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");

#[program]
pub mod rewards_pool {
    use super::*;

    /// §5.1 (no-base variant). `game_authority` signs via CPI AND pays rent (Q-13).
    pub fn open_pool(ctx: Context<OpenPool>, room_id: u64, payout_amounts: Vec<u64>) -> Result<()> {
        require!(payout_amounts.len() <= MAX_WINNERS, PocError::TooManyRanks);
        let pool = &mut ctx.accounts.match_pool;
        pool.rent_payer = ctx.accounts.game_authority.key();
        pool.version = 1;
        pool.game_program = ctx.accounts.game_program.key();
        pool.game_authority = ctx.accounts.game_authority.key();
        pool.room_id = room_id;
        pool.status = PoolStatus::Open;
        pool.fee_bps = 100;
        pool.opened_at = Clock::get()?.unix_timestamp;
        pool.payout_amounts = payout_amounts;
        pool.base_asset = None;
        pool.base_source = Pubkey::default();
        pool.winners = Vec::new();
        pool.bump = ctx.bumps.match_pool;
        Ok(())
    }

    /// §5.4. Never errors on state: returns `CpiOutcome` (Anchor return value).
    /// `match_pool` is an `UncheckedAccount` so a closed (empty) pool yields `NoOp(Closed)`.
    pub fn settle(ctx: Context<GameCall>, room_id: u64, winners: Vec<Option<Pubkey>>) -> Result<CpiOutcome> {
        let game_program = ctx.accounts.game_program.key();
        check_game_call(&ctx.accounts.game_authority.key(), &game_program, &ctx.accounts.match_pool.key(), room_id)?;
        let info = ctx.accounts.match_pool.to_account_info();
        if info.data_is_empty() {
            return Ok(CpiOutcome::NoOp(NoOpReason::Closed));
        }
        require_keys_eq!(*info.owner, crate::ID, PocError::WrongMatchPool);
        let mut pool = MatchPool::try_deserialize(&mut &info.try_borrow_data()?[..])?;
        require_keys_eq!(pool.game_authority, ctx.accounts.game_authority.key(), PocError::NotGameAuthority);
        let outcome = match pool.status {
            PoolStatus::Settled => CpiOutcome::NoOp(NoOpReason::AlreadySettled),
            PoolStatus::Cancelled => CpiOutcome::NoOp(NoOpReason::AlreadyCancelled),
            PoolStatus::Open | PoolStatus::BoostsLocked => {
                let mut winners = winners;
                winners.truncate(MAX_WINNERS);
                if winners.iter().all(|w| w.is_none()) {
                    pool.status = PoolStatus::Cancelled;
                    CpiOutcome::Cancelled(CancelReason::NoEligibleWinner)
                } else {
                    pool.winners = winners;
                    pool.status = PoolStatus::Settled;
                    CpiOutcome::Done
                }
            }
        };
        // UncheckedAccount is not written back automatically.
        let mut data = info.try_borrow_mut_data()?;
        pool.try_serialize(&mut &mut data[..])?;
        Ok(outcome)
    }

    /// §1.4 rule 3 / §3.2: vault = PDA token account created by Anchor `init` + `token::`.
    pub fn poc_init_vault(_ctx: Context<PocInitVault>) -> Result<()> {
        Ok(())
    }

    /// §3.4: returns the AssetKind the spec's predicate assigns to `mint`.
    pub fn poc_classify(ctx: Context<PocClassify>) -> Result<AssetKind> {
        Ok(classify(&ctx.accounts.mint))
    }

    /// §13 #5: `payer` (mut Signer) and `winner` (read-only) may be the same key.
    pub fn poc_dup_keys(ctx: Context<PocDupKeys>) -> Result<()> {
        msg!("payer={} winner={}", ctx.accounts.payer.key(), ctx.accounts.winner.key());
        Ok(())
    }

    /// Test helper: close the pool to its rent payer (reach the "account gone" state).
    pub fn poc_close_pool(_ctx: Context<PocClosePool>) -> Result<()> {
        Ok(())
    }
}

/// Address checks shared by every game-called instruction (§5 "pool ที่ถูกปิดแล้ว").
fn check_game_call(game_authority: &Pubkey, game_program: &Pubkey, match_pool: &Pubkey, room_id: u64) -> Result<()> {
    let expected_auth = Pubkey::find_program_address(&[GAME_AUTHORITY_SEED], game_program).0;
    require_keys_eq!(*game_authority, expected_auth, PocError::NotGameAuthority);
    let expected_pool = Pubkey::find_program_address(
        &[MATCH_POOL_SEED, game_program.as_ref(), &room_id.to_le_bytes()],
        &crate::ID,
    )
    .0;
    require_keys_eq!(*match_pool, expected_pool, PocError::WrongMatchPool);
    Ok(())
}

pub fn classify(mint: &InterfaceAccount<Mint>) -> AssetKind {
    let edition = Pubkey::find_program_address(
        &[b"metadata", TOKEN_METADATA_ID.as_ref(), mint.key().as_ref(), b"edition"],
        &TOKEN_METADATA_ID,
    )
    .0;
    let one_of_one = mint.decimals == 0 && mint.supply == 1;
    let fixed_supply = match mint.mint_authority {
        COption::None => true,
        COption::Some(a) => a == edition,
    };
    if one_of_one && fixed_supply {
        AssetKind::Nft
    } else {
        AssetKind::Fungible
    }
}

// ───────────────────────────── accounts ─────────────────────────────

#[derive(Accounts)]
#[instruction(room_id: u64)]
pub struct OpenPool<'info> {
    /// The game's PDA `[b"game_authority"]` — signer via CPI and rent payer (Q-13).
    #[account(mut, address = Pubkey::find_program_address(&[GAME_AUTHORITY_SEED], game_program.key).0 @ PocError::NotGameAuthority)]
    pub game_authority: Signer<'info>,
    /// CHECK: only used as a seed; the game_authority address check ties it to the signer.
    pub game_program: UncheckedAccount<'info>,
    #[account(
        init,
        payer = game_authority,
        space = 8 + MatchPool::INIT_SPACE,
        seeds = [MATCH_POOL_SEED, game_program.key().as_ref(), &room_id.to_le_bytes()],
        bump,
    )]
    pub match_pool: Account<'info, MatchPool>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct GameCall<'info> {
    pub game_authority: Signer<'info>,
    /// CHECK: seed only (checked in `check_game_call`).
    pub game_program: UncheckedAccount<'info>,
    /// CHECK: address, owner and discriminator checked by hand — may be empty (closed).
    #[account(mut)]
    pub match_pool: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct PocInitVault<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    #[account(mint::token_program = token_program)]
    pub mint: InterfaceAccount<'info, Mint>,
    /// Stand-in for the PoolAsset PDA (authority of the vault).
    /// CHECK: any key — POC only.
    pub owner: UncheckedAccount<'info>,
    #[account(
        init,
        payer = payer,
        seeds = [VAULT_SEED, owner.key().as_ref(), mint.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = owner,
        token::token_program = token_program,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,
    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct PocClassify<'info> {
    pub mint: InterfaceAccount<'info, Mint>,
}

#[derive(Accounts)]
pub struct PocDupKeys<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,
    /// CHECK: pinned destination stand-in; read-only.
    pub winner: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct PocClosePool<'info> {
    #[account(mut, close = rent_payer, has_one = rent_payer)]
    pub match_pool: Account<'info, MatchPool>,
    /// CHECK: pinned by has_one; receives the rent.
    #[account(mut)]
    pub rent_payer: UncheckedAccount<'info>,
}

// ───────────────────────────── state ─────────────────────────────

#[account]
#[derive(InitSpace)]
pub struct MatchPool {
    pub rent_payer: Pubkey,
    pub version: u8,
    pub game_program: Pubkey,
    pub game_authority: Pubkey,
    pub room_id: u64,
    pub status: PoolStatus,
    pub fee_bps: u16,
    pub opened_at: i64,
    #[max_len(MAX_WINNERS)]
    pub payout_amounts: Vec<u64>,
    pub base_asset: Option<Pubkey>,
    pub base_source: Pubkey,
    #[max_len(MAX_WINNERS)]
    pub winners: Vec<Option<Pubkey>>,
    pub boost_asset_count: u8,
    pub open_assets: u8,
    pub open_receipts: u32,
    pub bump: u8,
    pub _reserved: [u8; 64],
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug)]
pub enum PoolStatus {
    Open,
    BoostsLocked,
    Settled,
    Cancelled,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace, Debug)]
pub enum AssetKind {
    Sol,
    Fungible,
    Nft,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CpiOutcome {
    Done,
    NoOp(NoOpReason),
    Cancelled(CancelReason),
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum NoOpReason {
    AlreadyLocked,
    AlreadySettled,
    AlreadyCancelled,
    Closed,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CancelReason {
    ByGame,
    NoEligibleWinner,
    Expired,
}

#[error_code]
pub enum PocError {
    #[msg("signer is not the game_authority PDA of game_program")]
    NotGameAuthority,
    #[msg("too many ranks")]
    TooManyRanks,
    #[msg("match_pool is not the PDA of (game_program, room_id)")]
    WrongMatchPool,
}
