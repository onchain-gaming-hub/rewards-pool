//! Seeds and limits (spec-v2 §2). Seeds and enums are append-only — never change.
use anchor_lang::prelude::*;

pub const MATCH_POOL_SEED: &[u8] = b"match_pool";
pub const POOL_ASSET_SEED: &[u8] = b"pool_asset";
pub const VAULT_SEED: &[u8] = b"vault";
pub const BOOST_RECEIPT_SEED: &[u8] = b"boost";
/// The hub-wide PDA-signer seed of every game program (R1 · platform-core X5).
pub const GAME_AUTHORITY_SEED: &[u8] = b"game_authority";

// platform-core seeds (read-only consumer — 03 §2)
pub const PC_GAME_SEED: &[u8] = b"game";
pub const PC_CONFIG_SEED: &[u8] = b"config";
pub const PC_TREASURY_SEED: &[u8] = b"treasury";
pub const PC_BOOST_MINT_SEED: &[u8] = b"boost_mint";

/// MAX_SEATS (lobby = 8) ≤ MAX_WINNERS ≤ 32 (bitmap u32).
pub const MAX_WINNERS: usize = 32;
/// F6.
pub const MAX_BOOST_ASSETS: u8 = 8;
/// Bytes, not characters (Thai ≈ 3 bytes each).
pub const MAX_MESSAGE_BYTES: usize = 140;
/// Q-12.
pub const POOL_EXPIRY_SECS: i64 = 7 * 86_400;
/// F4: 0.001 SOL.
pub const MIN_SOL_BOOST: u64 = 1_000_000;
pub const ACCOUNT_VERSION: u8 = 1;
pub const BPS_DENOMINATOR: u64 = 10_000;
/// Token Metadata program — only used to derive the master edition PDA (§3.4).
pub const TOKEN_METADATA_ID: Pubkey = pubkey!("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s");
