//! §5.3–5.5 lock_boosts / settle / cancel — called by the game via CPI.
//! Never error on pool *state*: return `CpiOutcome` instead (an error would abort the
//! game's end-of-match tx). `match_pool` is an UncheckedAccount so a closed (empty)
//! pool yields `NoOp(Closed)`.
use crate::{
    constants::MAX_WINNERS,
    errors::RewardsError,
    events::{PoolCancelled, PoolLocked, PoolSettled},
    logic,
    state::*,
};
use anchor_lang::prelude::*;

#[derive(Accounts)]
pub struct GameCall<'info> {
    pub game_authority: Signer<'info>,
    /// CHECK: seed only — tied to game_authority in `logic::check_game_call`.
    pub game_program: UncheckedAccount<'info>,
    /// CHECK: address, owner and discriminator checked by hand; may be empty (closed).
    #[account(mut)]
    pub match_pool: UncheckedAccount<'info>,
}

/// Loads the pool, or returns the NoOp the game should see.
fn load(ctx: &Context<GameCall>, room_id: u64) -> Result<std::result::Result<MatchPool, CpiOutcome>> {
    let a = &ctx.accounts;
    logic::check_game_call(&a.game_authority.key(), &a.game_program.key(), &a.match_pool.key(), room_id)?;
    let info = a.match_pool.to_account_info();
    if info.data_is_empty() {
        return Ok(Err(CpiOutcome::NoOp(NoOpReason::Closed)));
    }
    require_keys_eq!(*info.owner, crate::ID, RewardsError::WrongMatchPool);
    let pool = MatchPool::try_deserialize(&mut &info.try_borrow_data()?[..])?;
    require_keys_eq!(pool.game_authority, a.game_authority.key(), RewardsError::NotGameAuthority);
    require_keys_eq!(pool.game_program, a.game_program.key(), RewardsError::WrongMatchPool);
    Ok(Ok(pool))
}

fn store(ctx: &Context<GameCall>, pool: &MatchPool) -> Result<()> {
    let info = ctx.accounts.match_pool.to_account_info();
    let mut data = info.try_borrow_mut_data()?;
    pool.try_serialize(&mut &mut data[..])
}

fn already(status: PoolStatus) -> CpiOutcome {
    CpiOutcome::NoOp(match status {
        PoolStatus::BoostsLocked => NoOpReason::AlreadyLocked,
        PoolStatus::Settled => NoOpReason::AlreadySettled,
        _ => NoOpReason::AlreadyCancelled,
    })
}

pub fn handle_lock_boosts(ctx: Context<GameCall>, room_id: u64) -> Result<CpiOutcome> {
    let mut pool = match load(&ctx, room_id)? {
        Ok(p) => p,
        Err(noop) => return Ok(noop),
    };
    if pool.status != PoolStatus::Open {
        return Ok(already(pool.status));
    }
    pool.status = PoolStatus::BoostsLocked;
    store(&ctx, &pool)?;
    emit!(PoolLocked { match_pool: ctx.accounts.match_pool.key() });
    Ok(CpiOutcome::Done)
}

pub fn handle_settle(ctx: Context<GameCall>, room_id: u64, winners: Vec<Option<Pubkey>>) -> Result<CpiOutcome> {
    let mut pool = match load(&ctx, room_id)? {
        Ok(p) => p,
        Err(noop) => return Ok(noop),
    };
    if !matches!(pool.status, PoolStatus::Open | PoolStatus::BoostsLocked) {
        return Ok(already(pool.status));
    }
    let key = ctx.accounts.match_pool.key();
    let mut winners = winners;
    winners.truncate(MAX_WINNERS);
    // Destinations that can never receive (System Program id, the pool itself) would make
    // that payout unclaimable forever and block close_pool → treat as "no eligible player".
    for w in winners.iter_mut() {
        if matches!(w, Some(k) if *k == Pubkey::default() || *k == key) {
            *w = None;
        }
    }
    let outcome = if winners.iter().all(|w| w.is_none()) {
        // R13: no eligible winner = cancel.
        pool.status = PoolStatus::Cancelled;
        emit!(PoolCancelled { match_pool: key, reason: CancelReason::NoEligibleWinner });
        CpiOutcome::Cancelled(CancelReason::NoEligibleWinner)
    } else {
        pool.winners = winners.clone();
        pool.status = PoolStatus::Settled;
        emit!(PoolSettled { match_pool: key, winners });
        CpiOutcome::Done
    };
    store(&ctx, &pool)?;
    Ok(outcome)
}

pub fn handle_cancel(ctx: Context<GameCall>, room_id: u64) -> Result<CpiOutcome> {
    let mut pool = match load(&ctx, room_id)? {
        Ok(p) => p,
        Err(noop) => return Ok(noop),
    };
    if !matches!(pool.status, PoolStatus::Open | PoolStatus::BoostsLocked) {
        // Never after Settled — claims may already have been paid.
        return Ok(already(pool.status));
    }
    pool.status = PoolStatus::Cancelled;
    store(&ctx, &pool)?;
    emit!(PoolCancelled { match_pool: ctx.accounts.match_pool.key(), reason: CancelReason::ByGame });
    Ok(CpiOutcome::Cancelled(CancelReason::ByGame))
}

// ─────────────── §5.12 expire_pool (anyone) ───────────────

#[derive(Accounts)]
pub struct ExpirePool<'info> {
    #[account(mut)]
    pub match_pool: Box<Account<'info, MatchPool>>,
}

pub fn handle_expire_pool(ctx: Context<ExpirePool>) -> Result<()> {
    let pool = &mut ctx.accounts.match_pool;
    require!(
        matches!(pool.status, PoolStatus::Open | PoolStatus::BoostsLocked),
        RewardsError::InvalidStatus
    );
    let deadline = pool.opened_at.checked_add(crate::constants::POOL_EXPIRY_SECS).ok_or(RewardsError::MathOverflow)?;
    require!(Clock::get()?.unix_timestamp >= deadline, RewardsError::NotExpired);
    pool.status = PoolStatus::Cancelled;
    emit!(PoolCancelled { match_pool: pool.key(), reason: CancelReason::Expired });
    Ok(())
}
