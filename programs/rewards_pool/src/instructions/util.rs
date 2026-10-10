use crate::constants::*;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, TransferChecked};

/// Moves `amount` out of a vault owned by the PoolAsset PDA.
#[allow(clippy::too_many_arguments)]
pub fn vault_transfer<'info>(
    token_program: AccountInfo<'info>,
    vault: AccountInfo<'info>,
    mint: AccountInfo<'info>,
    to: AccountInfo<'info>,
    pool_asset: AccountInfo<'info>,
    match_pool: Pubkey,
    asset_id: Pubkey,
    bump: u8,
    amount: u64,
    decimals: u8,
) -> Result<()> {
    let seeds: &[&[u8]] = &[POOL_ASSET_SEED, match_pool.as_ref(), asset_id.as_ref(), &[bump]];
    token_interface::transfer_checked(
        CpiContext::new_with_signer(
            token_program.key(),
            TransferChecked { from: vault, mint, to, authority: pool_asset },
            &[seeds],
        ),
        amount,
        decimals,
    )
}

/// Moves lamports out of a program-owned PoolAsset (SOL) — credit may go anywhere.
pub fn lamports_out(from: &AccountInfo, to: &AccountInfo, amount: u64) -> Result<()> {
    from.sub_lamports(amount)?;
    to.add_lamports(amount)?;
    Ok(())
}
