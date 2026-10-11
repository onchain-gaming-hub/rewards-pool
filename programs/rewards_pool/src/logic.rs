//! Pure money logic. Every instruction that moves value goes through
//! these functions so the conservation invariant has one definition.
use crate::{constants::*, errors::RewardsError, state::*};
use anchor_lang::{prelude::*, solana_program::program_option::COption};
use anchor_spl::token_interface::Mint;

/// NFT = 1/1 whose supply can never grow (no authority, or Token Metadata's
/// master edition PDA). Superset of platform-core `is_pure_one_of_one`.
pub fn classify(mint: &Mint, mint_key: &Pubkey) -> AssetKind {
    let edition = Pubkey::find_program_address(
        &[b"metadata", TOKEN_METADATA_ID.as_ref(), mint_key.as_ref(), b"edition"],
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

pub fn rank_one(pool: &MatchPool) -> Option<Pubkey> {
    pool.winners.first().copied().flatten()
}

pub fn winner_at(pool: &MatchPool, index: usize) -> Option<Pubkey> {
    pool.winners.get(index).copied().flatten()
}

pub fn is_base(pool: &MatchPool, asset: &PoolAsset) -> bool {
    pool.base_asset == Some(asset.asset_id)
}

/// Floor, once per asset, boost only, never on NFT, only when rank 1 is paid.
pub fn fee(pool: &MatchPool, asset: &PoolAsset) -> Result<u64> {
    if asset.kind == AssetKind::Nft || pool.status != PoolStatus::Settled || rank_one(pool).is_none() {
        return Ok(0);
    }
    fee_on(asset.boost_received, pool.fee_bps)
}

/// floor(amount × fee_bps / 10_000) in u128 — the one fee formula.
pub fn fee_on(amount: u64, fee_bps: u16) -> Result<u64> {
    let f = (amount as u128).checked_mul(fee_bps as u128).ok_or(RewardsError::MathOverflow)? / BPS_DENOMINATOR as u128;
    u64::try_from(f).map_err(|_| error!(RewardsError::MathOverflow))
}

/// What `winners[index]` receives of `asset`. 0 unless Settled.
pub fn payout(pool: &MatchPool, asset: &PoolAsset, index: usize) -> Result<u64> {
    if pool.status != PoolStatus::Settled || winner_at(pool, index).is_none() {
        return Ok(0);
    }
    let base = if is_base(pool, asset) {
        pool.payout_amounts.get(index).copied().unwrap_or(0)
    } else {
        0
    };
    let boost = if index == 0 {
        asset.boost_received.checked_sub(fee(pool, asset)?).ok_or(RewardsError::MathOverflow)?
    } else {
        0
    };
    base.checked_add(boost).ok_or_else(|| error!(RewardsError::MathOverflow))
}

/// Base that nobody receives (goes back to `base_source`).
pub fn unclaimed_base(pool: &MatchPool, asset: &PoolAsset) -> Result<u64> {
    if !is_base(pool, asset) {
        return Ok(0);
    }
    match pool.status {
        PoolStatus::Cancelled => Ok(asset.base_amount),
        PoolStatus::Settled => {
            let mut sum: u64 = 0;
            for (i, amt) in pool.payout_amounts.iter().enumerate() {
                if winner_at(pool, i).is_none() {
                    sum = sum.checked_add(*amt).ok_or(RewardsError::MathOverflow)?;
                }
            }
            Ok(sum)
        }
        _ => Ok(0),
    }
}

/// Boost goes back to boosters when the pool is Cancelled or has no rank 1.
pub fn boost_refundable(pool: &MatchPool) -> bool {
    match pool.status {
        PoolStatus::Cancelled => true,
        PoolStatus::Settled => rank_one(pool).is_none(),
        _ => false,
    }
}

/// Everything this asset owes has been paid, returned, collected or refunded.
pub fn asset_done(pool: &MatchPool, asset: &PoolAsset) -> Result<bool> {
    require_terminal(pool)?;
    for i in 0..pool.winners.len() {
        if payout(pool, asset, i)? > 0 && asset.claimed & (1u32 << i) == 0 {
            return Ok(false);
        }
    }
    if unclaimed_base(pool, asset)? > 0 && !asset.base_returned {
        return Ok(false);
    }
    if boost_refundable(pool) {
        if asset.boost_refunded != asset.boost_received {
            return Ok(false);
        }
    } else if !asset.fee_collected && fee(pool, asset)? > 0 {
        return Ok(false);
    }
    Ok(true)
}

/// Exits (refund / return / close) only once the pool is Settled or Cancelled.
pub fn require_terminal(pool: &MatchPool) -> Result<()> {
    require!(matches!(pool.status, PoolStatus::Settled | PoolStatus::Cancelled), RewardsError::InvalidStatus);
    Ok(())
}

/// Rent-exempt minimum of a 0-byte account.
pub fn min_rent() -> Result<u64> {
    Ok(Rent::get()?.minimum_balance(0))
}

/// Address checks for the game-called instructions (lock / settle / cancel).
pub fn check_game_call(game_authority: &Pubkey, game_program: &Pubkey, match_pool: &Pubkey, room_id: u64) -> Result<()> {
    let expected_auth = Pubkey::find_program_address(&[GAME_AUTHORITY_SEED], game_program).0;
    require_keys_eq!(*game_authority, expected_auth, RewardsError::NotGameAuthority);
    let expected_pool = Pubkey::find_program_address(
        &[MATCH_POOL_SEED, game_program.as_ref(), &room_id.to_le_bytes()],
        &crate::ID,
    )
    .0;
    require_keys_eq!(*match_pool, expected_pool, RewardsError::WrongMatchPool);
    Ok(())
}
