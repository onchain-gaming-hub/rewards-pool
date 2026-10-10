//! §5.1 open_pool / open_pool_sol / open_pool_token — called by the game via CPI.
use crate::{
    constants::*, errors::RewardsError, events::PoolOpened, logic, platform_core, state::*,
};
use anchor_lang::{prelude::*, system_program};
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked},
};
use platform_core::{
    accounts::{AllowedMint, GameEntry, PlatformConfig},
    types::GameStatus,
};

fn validate_payouts(payout_amounts: &[u64], min_each: u64) -> Result<u64> {
    require!(!payout_amounts.is_empty(), RewardsError::InvalidPayoutAmount);
    require!(payout_amounts.len() <= MAX_WINNERS, RewardsError::TooManyRanks);
    let mut sum: u64 = 0;
    for a in payout_amounts {
        require!(*a > 0, RewardsError::InvalidPayoutAmount);
        require!(*a >= min_each, RewardsError::PayoutBelowRentExempt);
        sum = sum.checked_add(*a).ok_or(RewardsError::InvalidPayoutAmount)?;
    }
    Ok(sum)
}

#[allow(clippy::too_many_arguments)]
fn init_pool(
    pool: &mut MatchPool,
    key: Pubkey,
    game_authority: Pubkey,
    game_program: Pubkey,
    room_id: u64,
    fee_bps: u16,
    bump: u8,
    payout_amounts: Vec<u64>,
    base_asset: Option<Pubkey>,
    base_source: Pubkey,
) -> Result<()> {
    pool.rent_payer = game_authority;
    pool.version = ACCOUNT_VERSION;
    pool.game_program = game_program;
    pool.game_authority = game_authority;
    pool.room_id = room_id;
    pool.status = PoolStatus::Open;
    pool.fee_bps = fee_bps;
    pool.opened_at = Clock::get()?.unix_timestamp;
    pool.payout_amounts = payout_amounts.clone();
    pool.base_asset = base_asset;
    pool.base_source = base_source;
    pool.winners = Vec::new();
    pool.boost_asset_count = 0;
    pool.open_assets = if base_asset.is_some() { 1 } else { 0 };
    pool.open_receipts = 0;
    pool.bump = bump;
    emit!(PoolOpened { match_pool: key, game_program, room_id, fee_bps, base_asset, payout_amounts });
    Ok(())
}

// ─────────────── no base ───────────────

#[derive(Accounts)]
#[instruction(room_id: u64)]
pub struct OpenPool<'info> {
    /// The game's PDA signer (CPI) and rent payer (Q-13). System-owned, holds SOL.
    #[account(mut, address = game_entry.game_authority @ RewardsError::NotGameAuthority)]
    pub game_authority: Signer<'info>,
    /// CHECK: seed of game_entry / match_pool — bound to game_entry by its seeds.
    pub game_program: UncheckedAccount<'info>,
    #[account(
        seeds = [PC_GAME_SEED, game_program.key().as_ref()],
        bump = game_entry.bump,
        seeds::program = platform_core::ID,
        constraint = matches!(game_entry.status, GameStatus::Active) @ RewardsError::GameNotActive,
    )]
    pub game_entry: Box<Account<'info, GameEntry>>,
    #[account(seeds = [PC_CONFIG_SEED], bump = config.bump, seeds::program = platform_core::ID)]
    pub config: Box<Account<'info, PlatformConfig>>,
    #[account(
        init,
        payer = game_authority,
        space = 8 + MatchPool::INIT_SPACE,
        seeds = [MATCH_POOL_SEED, game_program.key().as_ref(), &room_id.to_le_bytes()],
        bump,
    )]
    pub match_pool: Box<Account<'info, MatchPool>>,
    pub system_program: Program<'info, System>,
}

pub fn handle_open_pool(ctx: Context<OpenPool>, room_id: u64) -> Result<()> {
    let key = ctx.accounts.match_pool.key();
    init_pool(
        &mut ctx.accounts.match_pool,
        key,
        ctx.accounts.game_authority.key(),
        ctx.accounts.game_program.key(),
        room_id,
        ctx.accounts.config.boost_fee_bps,
        ctx.bumps.match_pool,
        Vec::new(),
        None,
        Pubkey::default(),
    )
}

// ─────────────── SOL base ───────────────

#[derive(Accounts)]
#[instruction(room_id: u64)]
pub struct OpenPoolSol<'info> {
    #[account(mut, address = game_entry.game_authority @ RewardsError::NotGameAuthority)]
    pub game_authority: Signer<'info>,
    /// CHECK: see OpenPool.
    pub game_program: UncheckedAccount<'info>,
    #[account(
        seeds = [PC_GAME_SEED, game_program.key().as_ref()],
        bump = game_entry.bump,
        seeds::program = platform_core::ID,
        constraint = matches!(game_entry.status, GameStatus::Active) @ RewardsError::GameNotActive,
    )]
    pub game_entry: Box<Account<'info, GameEntry>>,
    #[account(seeds = [PC_CONFIG_SEED], bump = config.bump, seeds::program = platform_core::ID)]
    pub config: Box<Account<'info, PlatformConfig>>,
    #[account(
        init,
        payer = game_authority,
        space = 8 + MatchPool::INIT_SPACE,
        seeds = [MATCH_POOL_SEED, game_program.key().as_ref(), &room_id.to_le_bytes()],
        bump,
    )]
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(
        init,
        payer = game_authority,
        space = 8 + PoolAsset::INIT_SPACE,
        seeds = [POOL_ASSET_SEED, match_pool.key().as_ref(), Pubkey::default().as_ref()],
        bump,
    )]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    pub system_program: Program<'info, System>,
}

pub fn handle_open_pool_sol(ctx: Context<OpenPoolSol>, room_id: u64, payout_amounts: Vec<u64>) -> Result<()> {
    let total = validate_payouts(&payout_amounts, logic::min_rent()?)?; // F9 (a)
    let ga = ctx.accounts.game_authority.key();
    system_program::transfer(
        CpiContext::new(
            system_program::ID,
            system_program::Transfer {
                from: ctx.accounts.game_authority.to_account_info(),
                to: ctx.accounts.pool_asset.to_account_info(),
            },
        ),
        total,
    )?;
    let pool_key = ctx.accounts.match_pool.key();
    let a = &mut ctx.accounts.pool_asset;
    a.rent_payer = ga;
    a.match_pool = pool_key;
    a.version = ACCOUNT_VERSION;
    a.kind = AssetKind::Sol;
    a.asset_id = Pubkey::default();
    a.token_program = Pubkey::default();
    a.decimals = 9;
    a.base_amount = total;
    a.bump = ctx.bumps.pool_asset;
    init_pool(
        &mut ctx.accounts.match_pool,
        pool_key,
        ga,
        ctx.accounts.game_program.key(),
        room_id,
        ctx.accounts.config.boost_fee_bps,
        ctx.bumps.match_pool,
        payout_amounts,
        Some(Pubkey::default()),
        ga, // SOL base returns to game_authority
    )
}

// ─────────────── token base ───────────────

#[derive(Accounts)]
#[instruction(room_id: u64)]
pub struct OpenPoolToken<'info> {
    #[account(mut, address = game_entry.game_authority @ RewardsError::NotGameAuthority)]
    pub game_authority: Signer<'info>,
    /// CHECK: see OpenPool.
    pub game_program: UncheckedAccount<'info>,
    #[account(
        seeds = [PC_GAME_SEED, game_program.key().as_ref()],
        bump = game_entry.bump,
        seeds::program = platform_core::ID,
        constraint = matches!(game_entry.status, GameStatus::Active) @ RewardsError::GameNotActive,
    )]
    pub game_entry: Box<Account<'info, GameEntry>>,
    #[account(seeds = [PC_CONFIG_SEED], bump = config.bump, seeds::program = platform_core::ID)]
    pub config: Box<Account<'info, PlatformConfig>>,
    #[account(
        init,
        payer = game_authority,
        space = 8 + MatchPool::INIT_SPACE,
        seeds = [MATCH_POOL_SEED, game_program.key().as_ref(), &room_id.to_le_bytes()],
        bump,
    )]
    pub match_pool: Box<Account<'info, MatchPool>>,
    #[account(mint::token_program = token_program)]
    pub base_mint: Box<InterfaceAccount<'info, Mint>>,
    /// F5: base mint must be allowlisted (extension filter + Treasury ATA exist).
    #[account(
        seeds = [PC_BOOST_MINT_SEED, base_mint.key().as_ref()],
        bump = allowed_mint.bump,
        seeds::program = platform_core::ID,
        constraint = allowed_mint.token_program == token_program.key() @ RewardsError::TokenProgramMismatch,
    )]
    pub allowed_mint: Box<Account<'info, AllowedMint>>,
    /// ATA of game_authority — re-creatable at the same address, so return_base always works.
    #[account(
        mut,
        associated_token::mint = base_mint,
        associated_token::authority = game_authority,
        associated_token::token_program = token_program,
    )]
    pub base_source: Box<InterfaceAccount<'info, TokenAccount>>,
    #[account(
        init,
        payer = game_authority,
        space = 8 + PoolAsset::INIT_SPACE,
        seeds = [POOL_ASSET_SEED, match_pool.key().as_ref(), base_mint.key().as_ref()],
        bump,
    )]
    pub pool_asset: Box<Account<'info, PoolAsset>>,
    /// Anchor `init` + `token::` — sized for the mint's required account extensions and
    /// tolerant of a pre-funded address (POC §13 #3).
    #[account(
        init,
        payer = game_authority,
        seeds = [VAULT_SEED, pool_asset.key().as_ref()],
        bump,
        token::mint = base_mint,
        token::authority = pool_asset,
        token::token_program = token_program,
    )]
    pub vault: Box<InterfaceAccount<'info, TokenAccount>>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

pub fn handle_open_pool_token(ctx: Context<OpenPoolToken>, room_id: u64, payout_amounts: Vec<u64>) -> Result<()> {
    let total = validate_payouts(&payout_amounts, 1)?;
    let mint_key = ctx.accounts.base_mint.key();
    require!(
        logic::classify(&ctx.accounts.base_mint, &mint_key) == AssetKind::Fungible,
        RewardsError::BaseMustBeFungible
    );
    let decimals = ctx.accounts.base_mint.decimals;
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.base_source.to_account_info(),
                mint: ctx.accounts.base_mint.to_account_info(),
                to: ctx.accounts.vault.to_account_info(),
                authority: ctx.accounts.game_authority.to_account_info(),
            },
        ),
        total,
        decimals,
    )?;
    ctx.accounts.vault.reload()?;
    require!(ctx.accounts.vault.amount == total, RewardsError::BaseAmountMismatch);

    let pool_key = ctx.accounts.match_pool.key();
    let ga = ctx.accounts.game_authority.key();
    let a = &mut ctx.accounts.pool_asset;
    a.rent_payer = ga;
    a.match_pool = pool_key;
    a.version = ACCOUNT_VERSION;
    a.kind = AssetKind::Fungible;
    a.asset_id = mint_key;
    a.token_program = ctx.accounts.token_program.key();
    a.decimals = decimals;
    a.base_amount = total;
    a.bump = ctx.bumps.pool_asset;
    a.vault_bump = ctx.bumps.vault;
    let base_source = ctx.accounts.base_source.key();
    init_pool(
        &mut ctx.accounts.match_pool,
        pool_key,
        ga,
        ctx.accounts.game_program.key(),
        room_id,
        ctx.accounts.config.boost_fee_bps,
        ctx.bumps.match_pool,
        payout_amounts,
        Some(mint_key),
        base_source,
    )
}
