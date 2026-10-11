//! Accounts. Every account: discriminator · `rent_payer` at offset 8 ·
//! `version` · `_reserved`. Never reorder fields, seeds or enum variants.
use crate::constants::*;
use anchor_lang::prelude::*;

/// `[b"match_pool", game_program, room_id.to_le_bytes()]`
#[account]
#[derive(InitSpace)]
pub struct MatchPool {
    /// offset 8 · = game_authority (Q-13 / D11).
    pub rent_payer: Pubkey,
    pub version: u8,
    pub game_program: Pubkey,
    /// Snapshot of GameEntry.game_authority at open (never changes — 03 §2.2 X5).
    pub game_authority: Pubkey,
    pub room_id: u64,
    pub status: PoolStatus,
    /// Snapshot of PlatformConfig.boost_fee_bps at open.
    pub fee_bps: u16,
    pub opened_at: i64,
    #[max_len(MAX_WINNERS)]
    pub payout_amounts: Vec<u64>,
    /// None = no base · SOL base = Some(Pubkey::default()).
    pub base_asset: Option<Pubkey>,
    /// token base = ATA of game_authority · SOL base = game_authority · none = default.
    pub base_source: Pubkey,
    /// index 0 = rank 1 (receives every boost) · None = bot / disconnected / AI agent.
    #[max_len(MAX_WINNERS)]
    pub winners: Vec<Option<Pubkey>>,
    pub boost_asset_count: u8,
    pub open_assets: u8,
    pub open_receipts: u32,
    pub bump: u8,
    pub _reserved: [u8; 64],
}

/// `[b"pool_asset", match_pool, asset_id]`
#[account]
#[derive(InitSpace)]
pub struct PoolAsset {
    /// offset 8 · base: game_authority · boost: first booster of this asset.
    pub rent_payer: Pubkey,
    /// offset 40.
    pub match_pool: Pubkey,
    pub version: u8,
    pub kind: AssetKind,
    /// mint · SOL = Pubkey::default().
    pub asset_id: Pubkey,
    pub token_program: Pubkey,
    pub decimals: u8,
    pub base_amount: u64,
    pub boost_received: u64,
    /// bit i = winners[i] claimed this asset.
    pub claimed: u32,
    pub base_returned: bool,
    pub boost_refunded: u64,
    pub fee_collected: bool,
    pub bump: u8,
    pub vault_bump: u8,
    pub _reserved: [u8; 32],
}

/// `[b"boost", match_pool, booster, asset_id]`
#[account]
#[derive(InitSpace)]
pub struct BoostReceipt {
    /// offset 8 · = booster (W21).
    pub rent_payer: Pubkey,
    /// offset 40.
    pub match_pool: Pubkey,
    pub version: u8,
    pub booster: Pubkey,
    pub asset_id: Pubkey,
    pub amount: u64,
    pub bump: u8,
    pub _reserved: [u8; 16],
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

/// Returned (Anchor return value) by every instruction a game CPIs into.
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
    /// match_pool has no data (closed, or never opened).
    Closed,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CancelReason {
    ByGame,
    NoEligibleWinner,
    Expired,
}
