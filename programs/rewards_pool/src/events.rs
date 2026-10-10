//! Events (`emit!` — Q-9). Program log: may be truncated / not retained by every RPC.
use crate::state::CancelReason;
use anchor_lang::prelude::*;

#[event]
pub struct PoolOpened {
    pub match_pool: Pubkey,
    pub game_program: Pubkey,
    pub room_id: u64,
    pub fee_bps: u16,
    pub base_asset: Option<Pubkey>,
    pub payout_amounts: Vec<u64>,
}
#[event]
pub struct BoostAdded {
    pub match_pool: Pubkey,
    pub booster: Pubkey,
    pub asset_id: Pubkey,
    pub amount: u64,
    pub message: String,
}
#[event]
pub struct PoolLocked {
    pub match_pool: Pubkey,
}
#[event]
pub struct PoolSettled {
    pub match_pool: Pubkey,
    pub winners: Vec<Option<Pubkey>>,
}
#[event]
pub struct PoolCancelled {
    pub match_pool: Pubkey,
    pub reason: CancelReason,
}
#[event]
pub struct Claimed {
    pub match_pool: Pubkey,
    pub index: u8,
    pub player: Pubkey,
    pub asset_id: Pubkey,
    pub amount: u64,
}
#[event]
pub struct FeeCollected {
    pub match_pool: Pubkey,
    pub asset_id: Pubkey,
    pub fee: u64,
}
#[event]
pub struct BaseReturned {
    pub match_pool: Pubkey,
    pub asset_id: Pubkey,
    pub amount: u64,
    pub to: Pubkey,
}
#[event]
pub struct BoostRefunded {
    pub match_pool: Pubkey,
    pub booster: Pubkey,
    pub asset_id: Pubkey,
    pub amount: u64,
}
#[event]
pub struct PoolClosed {
    pub match_pool: Pubkey,
    pub rent_payer: Pubkey,
}
