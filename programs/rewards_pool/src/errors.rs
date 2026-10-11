//! Error codes — append-only (6000 + index).
use anchor_lang::prelude::*;

#[error_code]
pub enum RewardsError {
    #[msg("signer is not the game_authority of this game")]
    NotGameAuthority, // 6000
    #[msg("game is not Active in platform-core")]
    GameNotActive,
    #[msg("more ranks than MAX_WINNERS")]
    TooManyRanks,
    #[msg("payout amount is 0, the list is empty, or the sum overflows")]
    InvalidPayoutAmount,
    #[msg("base deposited into the vault differs from the sum of payout_amounts")]
    BaseAmountMismatch,
    #[msg("pool is not open for boosts")]
    PoolNotOpen, // 6005
    #[msg("pool already holds MAX_BOOST_ASSETS boosted assets")]
    TooManyAssets,
    #[msg("message longer than MAX_MESSAGE_BYTES")]
    MessageTooLong,
    #[msg("SOL boost below MIN_SOL_BOOST")]
    BoostTooSmall,
    #[msg("nothing was deposited")]
    ZeroDeposit,
    #[msg("an NFT boost must be exactly 1")]
    InvalidNftAmount, // 6010
    #[msg("token program does not match the mint / pool asset")]
    TokenProgramMismatch,
    #[msg("pool is not settled")]
    PoolNotSettled,
    #[msg("no eligible winner at this index")]
    NoWinnerAtIndex,
    #[msg("nothing to claim for this index and asset")]
    NothingToClaim,
    #[msg("already claimed")]
    AlreadyClaimed, // 6015
    #[msg("NFTs carry no platform fee")]
    NoFeeForNft,
    #[msg("fee already collected")]
    FeeAlreadyCollected,
    #[msg("base already returned")]
    BaseAlreadyReturned,
    #[msg("boost refund is not allowed in this state")]
    RefundNotAllowed,
    #[msg("pool has not reached POOL_EXPIRY")]
    NotExpired, // 6020
    #[msg("instruction not allowed in the pool's current status")]
    InvalidStatus,
    #[msg("asset still owes claims, fee, base return or refunds")]
    AssetNotSettled,
    #[msg("pool still has open assets or receipts")]
    PoolNotEmpty,
    #[msg("math overflow")]
    MathOverflow,
    #[msg("base asset must be fungible")]
    BaseMustBeFungible, // 6025
    #[msg("base accounts do not match")]
    BaseAccountsMismatch,
    #[msg("SOL payout below rent-exempt minimum")]
    PayoutBelowRentExempt,
    #[msg("no eligible rank-1 winner")]
    NoRankOneWinner,
    #[msg("receipt does not belong to this pool / asset")]
    ReceiptAssetMismatch,
    #[msg("SOL / token variant does not match the pool asset kind")]
    AssetKindMismatch, // 6030
    #[msg("SOL boost net of fee is below rent-exempt minimum")]
    BoostNetBelowRentExempt,
    #[msg("asset is not the pool's base asset")]
    NotBaseAsset,
    #[msg("match_pool is not the PDA of (game_program, room_id)")]
    WrongMatchPool, // 6033
    #[msg("snapshotted boost fee above MAX_FEE_BPS")]
    FeeTooHigh, // 6034
}
