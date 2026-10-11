//! End-to-end money flows (conservation invariant). Every test ends with
//! every rewards-pool account closed and every unit accounted for.
mod common;
use {
    anchor_lang::prelude::Pubkey,
    common::*,
    rewards_pool::state::{AssetKind, PoolStatus},
    solana_keypair::Keypair,
    solana_signer::Signer,
};

const BASE: [u64; 3] = [10_000_000, 5_000_000, 2_000_000]; // SOL, each ≥ rent-exempt

fn fee(x: u64) -> u64 {
    x * FEE_BPS as u64 / 10_000
}

/// (a) SOL base + SOL boost + token boost + NFT boost · winners [A, None, C].
#[test]
fn full_flow_sol_base_with_all_boost_kinds() {
    let mut w = world();
    let (a, c) = (Keypair::new(), Keypair::new());
    let (b_sol, b_tok, b_nft) = (funded(&mut w.svm), funded(&mut w.svm), funded(&mut w.svm));
    let tok = fungible(&mut w.svm, &w.payer, &b_tok.pubkey(), &TOKEN, 1_000_000);
    let nft_mint = nft(&mut w.svm, &w.payer, &b_nft.pubkey());
    create_ata(&mut w.svm, &w.payer, &a.pubkey(), &tok, &TOKEN);
    create_ata(&mut w.svm, &w.payer, &a.pubkey(), &nft_mint, &TOKEN);
    let ga0 = lamports(&w.svm, &game_authority());

    ok(send(&mut w.svm, &[start_match_sol(1, BASE.to_vec())], &[&w.payer]));
    let p = pool_pda(1);
    let sol_asset = asset_pda(&p, &Pubkey::default());
    assert_eq!(pool(&w.svm, 1).fee_bps, FEE_BPS, "fee snapshot");
    assert_eq!(pool(&w.svm, 1).open_assets, 1);

    ok(send(&mut w.svm, &[boost_sol(1, &b_sol.pubkey(), SOL / 10, "สู้ๆ นะ")], &[&b_sol]));
    ok(send(&mut w.svm, &[boost_token(1, &b_tok.pubkey(), &tok, &TOKEN, 1_000, "go")], &[&b_tok]));
    ok(send(&mut w.svm, &[boost_token(1, &b_nft.pubkey(), &nft_mint, &TOKEN, 1, "nft!")], &[&b_nft]));
    let pl = pool(&w.svm, 1);
    assert_eq!((pl.open_assets, pl.open_receipts, pl.boost_asset_count), (3, 3, 2), "SOL base asset reused by the SOL boost");
    assert_eq!(asset(&w.svm, &asset_pda(&p, &nft_mint)).kind, AssetKind::Nft);

    assert_eq!(outcome(send(&mut w.svm, &[finish(1, vec![Some(a.pubkey()), None, Some(c.pubkey())])], &[&w.payer])), 0);
    assert_eq!(pool(&w.svm, 1).status, PoolStatus::Settled);

    let treasury = treasury_pda().0;
    let t0 = lamports(&w.svm, &treasury);
    ok(send(&mut w.svm, &[collect_fee_sol(1), collect_fee_token(1, &tok, &TOKEN)], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &treasury) - t0, fee(SOL / 10));
    assert_eq!(token_balance(&w.svm, &ata(&treasury, &tok, &TOKEN)), fee(1_000));

    // Claim All for A in one tx: SOL + token + NFT.
    ok(send(&mut w.svm, &[claim_sol(1, 0, &a.pubkey()), claim_token(1, 0, &a.pubkey(), &tok, &TOKEN), claim_token(1, 0, &a.pubkey(), &nft_mint, &TOKEN)], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &a.pubkey()), BASE[0] + SOL / 10 - fee(SOL / 10));
    assert_eq!(token_balance(&w.svm, &ata(&a.pubkey(), &tok, &TOKEN)), 1_000 - fee(1_000));
    assert_eq!(token_balance(&w.svm, &ata(&a.pubkey(), &nft_mint, &TOKEN)), 1);
    ok(send(&mut w.svm, &[claim_sol(1, 2, &c.pubkey())], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &c.pubkey()), BASE[2]);

    // Index 1 was a bot → its base goes back to game_authority.
    ok(send(&mut w.svm, &[return_base_sol(1)], &[&w.payer]));
    for (bkr, id) in [(&b_sol, Pubkey::default()), (&b_tok, tok), (&b_nft, nft_mint)] {
        ok(send(&mut w.svm, &[close_receipt(1, &bkr.pubkey(), &id)], &[&w.payer]));
    }
    ok(send(&mut w.svm, &[close_sol_asset(1, &game_authority())], &[&w.payer]));
    ok(send(&mut w.svm, &[close_token_asset(1, &tok, &TOKEN, &b_tok.pubkey()), close_token_asset(1, &nft_mint, &TOKEN, &b_nft.pubkey())], &[&w.payer]));
    ok(send(&mut w.svm, &[close_pool(1)], &[&w.payer]));

    for acc in [p, sol_asset, asset_pda(&p, &tok), vault_pda(&asset_pda(&p, &tok)), asset_pda(&p, &nft_mint)] {
        assert!(gone(&w.svm, &acc), "{acc} closed");
    }
    // game_authority: paid base A + C, got base[1] back, and all rent back.
    assert_eq!(ga0 - lamports(&w.svm, &game_authority()), BASE[0] + BASE[2], "only the paid base left the game");
}

/// Base token == boost token (one PoolAsset) · fee only on the boost part.
#[test]
fn token_base_and_same_mint_boost_share_one_asset() {
    let mut w = world();
    let (a, b) = (Keypair::new(), Keypair::new());
    let booster = funded(&mut w.svm);
    let point = fungible(&mut w.svm, &w.payer, &game_authority(), &TOKEN_2022, 10_000);
    create_ata(&mut w.svm, &w.payer, &booster.pubkey(), &point, &TOKEN_2022);
    mint_to(&mut w.svm, &w.payer, &point, &ata(&booster.pubkey(), &point, &TOKEN_2022), &TOKEN_2022, 5_000);
    for x in [&a, &b] {
        create_ata(&mut w.svm, &w.payer, &x.pubkey(), &point, &TOKEN_2022);
    }
    ok(send(&mut w.svm, &[start_match_token(2, &point, &TOKEN_2022, vec![600, 300, 100])], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(2, &booster.pubkey(), &point, &TOKEN_2022, 2_000, "")], &[&booster]));
    let pa = asset_pda(&pool_pda(2), &point);
    let st = asset(&w.svm, &pa);
    assert_eq!((st.base_amount, st.boost_received), (1_000, 2_000));
    assert_eq!(pool(&w.svm, 2).boost_asset_count, 0, "boosting the base mint does not use a boost slot");

    assert_eq!(outcome(send(&mut w.svm, &[finish(2, vec![Some(a.pubkey()), Some(b.pubkey())])], &[&w.payer])), 0);
    ok(send(&mut w.svm, &[collect_fee_token(2, &point, &TOKEN_2022)], &[&w.payer]));
    ok(send(&mut w.svm, &[claim_token(2, 0, &a.pubkey(), &point, &TOKEN_2022), claim_token(2, 1, &b.pubkey(), &point, &TOKEN_2022)], &[&w.payer]));
    assert_eq!(token_balance(&w.svm, &ata(&a.pubkey(), &point, &TOKEN_2022)), 600 + 2_000 - fee(2_000));
    assert_eq!(token_balance(&w.svm, &ata(&b.pubkey(), &point, &TOKEN_2022)), 300);
    let ga_ata = ata(&game_authority(), &point, &TOKEN_2022);
    let before = token_balance(&w.svm, &ga_ata);
    ok(send(&mut w.svm, &[return_base_token(2, &point, &TOKEN_2022)], &[&w.payer]));
    assert_eq!(token_balance(&w.svm, &ga_ata) - before, 100, "rank 3 had no player → returned");
    ok(send(&mut w.svm, &[close_receipt(2, &booster.pubkey(), &point), close_token_asset(2, &point, &TOKEN_2022, &game_authority()), close_pool(2)], &[&w.payer]));
    assert!(gone(&w.svm, &pa) && gone(&w.svm, &pool_pda(2)));
}

/// (b) rank 1 is a bot → every boost is refunded in full (no fee) · rank 2 still gets base.
#[test]
fn no_rank_one_refunds_all_boosts_without_fee() {
    let mut w = world();
    let b = Keypair::new();
    let (b_sol, b_nft) = (funded(&mut w.svm), funded(&mut w.svm));
    let nft_mint = nft(&mut w.svm, &w.payer, &b_nft.pubkey());
    ok(send(&mut w.svm, &[start_match_sol(3, BASE.to_vec())], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_sol(3, &b_sol.pubkey(), SOL / 10, ""), ], &[&b_sol]));
    ok(send(&mut w.svm, &[boost_token(3, &b_nft.pubkey(), &nft_mint, &TOKEN, 1, "")], &[&b_nft]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(3, vec![None, Some(b.pubkey())])], &[&w.payer])), 0);

    assert_err(&send(&mut w.svm, &[collect_fee_sol(3)], &[&w.payer]), 6028); // NoRankOneWinner
    let s0 = lamports(&w.svm, &b_sol.pubkey());
    ok(send(&mut w.svm, &[refund_sol(3, &b_sol.pubkey())], &[&w.payer]));
    let receipt_rent = w.svm.minimum_balance_for_rent_exemption(8 + 32 + 32 + 1 + 32 + 32 + 8 + 1 + 16);
    assert_eq!(lamports(&w.svm, &b_sol.pubkey()) - s0, SOL / 10 + receipt_rent, "full boost + receipt rent back");
    ok(send(&mut w.svm, &[refund_token(3, &b_nft.pubkey(), &nft_mint, &TOKEN)], &[&w.payer]));
    assert_eq!(token_balance(&w.svm, &ata(&b_nft.pubkey(), &nft_mint, &TOKEN)), 1, "NFT back to booster");

    ok(send(&mut w.svm, &[claim_sol(3, 1, &b.pubkey()), return_base_sol(3)], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &b.pubkey()), BASE[1]);
    ok(send(&mut w.svm, &[close_sol_asset(3, &game_authority()), close_token_asset(3, &nft_mint, &TOKEN, &b_nft.pubkey()), close_pool(3)], &[&w.payer]));
    assert!(gone(&w.svm, &pool_pda(3)));
}

/// (c) cancel → full base back, full boost back · game CPIs afterwards are NoOps.
#[test]
fn cancel_returns_everything_and_later_game_calls_are_noops() {
    let mut w = world();
    let booster = funded(&mut w.svm);
    let ga0 = lamports(&w.svm, &game_authority());
    ok(send(&mut w.svm, &[start_match_sol(4, BASE.to_vec())], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_sol(4, &booster.pubkey(), SOL, "")], &[&booster]));
    assert_eq!(outcome(send(&mut w.svm, &[cancel_match(4)], &[&w.payer])), 20); // Cancelled(ByGame)
    assert_eq!(outcome(send(&mut w.svm, &[finish(4, vec![Some(booster.pubkey())])], &[&w.payer])), 12); // NoOp(AlreadyCancelled)
    assert_eq!(outcome(send(&mut w.svm, &[lock(4)], &[&w.payer])), 12);
    let b0 = lamports(&w.svm, &booster.pubkey());
    let receipt_rent = lamports(&w.svm, &receipt_pda(&pool_pda(4), &booster.pubkey(), &Pubkey::default()));
    ok(send(&mut w.svm, &[refund_sol(4, &booster.pubkey()), return_base_sol(4), close_sol_asset(4, &game_authority()), close_pool(4)], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &booster.pubkey()) - b0, SOL + receipt_rent, "booster refunded in full + receipt rent");
    assert_eq!(lamports(&w.svm, &game_authority()), ga0, "game_authority made whole (base + all rent)");
    assert_eq!(outcome(send(&mut w.svm, &[finish(4, vec![Some(booster.pubkey())])], &[&w.payer])), 13, "closed pool → NoOp(Closed)");
}

/// Expire after 7 days → refunds · settle afterwards = NoOp.
#[test]
fn expire_pool_after_seven_days() {
    let mut w = world();
    let booster = funded(&mut w.svm);
    set_time(&mut w.svm, 1_000_000);
    ok(send(&mut w.svm, &[start_match(5)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_sol(5, &booster.pubkey(), SOL / 100, "")], &[&booster]));
    set_time(&mut w.svm, 1_000_000 + 7 * 86_400 - 1);
    assert_err(&send(&mut w.svm, &[expire(5)], &[&w.payer]), 6020); // NotExpired
    set_time(&mut w.svm, 1_000_000 + 7 * 86_400);
    ok(send(&mut w.svm, &[expire(5)], &[&w.payer]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(5, vec![Some(booster.pubkey())])], &[&w.payer])), 12);
    ok(send(&mut w.svm, &[refund_sol(5, &booster.pubkey()), close_sol_asset(5, &booster.pubkey()), close_pool(5)], &[&w.payer]));
    assert!(gone(&w.svm, &pool_pda(5)));
}

/// settle with only bots → Cancelled(NoEligibleWinner), Ok (never errors in the game's tx).
#[test]
fn all_none_winners_cancel_inside_settle() {
    let mut w = world();
    ok(send(&mut w.svm, &[start_match(6)], &[&w.payer]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(6, vec![None, None])], &[&w.payer])), 21);
    assert_eq!(outcome(send(&mut w.svm, &[finish(6, vec![])], &[&w.payer])), 12, "already cancelled");
    assert_eq!(pool(&w.svm, 6).status, PoolStatus::Cancelled);
}

/// Game Suspended / mint disallowed after the pool started: nothing gets locked.
#[test]
fn suspend_and_disallow_after_start_do_not_lock_funds() {
    let mut w = world();
    let a = Keypair::new();
    let booster = funded(&mut w.svm);
    let tok = fungible(&mut w.svm, &w.payer, &booster.pubkey(), &TOKEN, 1_000);
    create_ata(&mut w.svm, &w.payer, &a.pubkey(), &tok, &TOKEN);
    ok(send(&mut w.svm, &[start_match(7)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(7, &booster.pubkey(), &tok, &TOKEN, 500, "")], &[&booster]));
    put_game_entry(&mut w.svm, true); // Suspended
    disallow_mint(&mut w.svm, &tok);
    assert_eq!(outcome(send(&mut w.svm, &[finish(7, vec![Some(a.pubkey())])], &[&w.payer])), 0);
    ok(send(&mut w.svm, &[collect_fee_token(7, &tok, &TOKEN), claim_token(7, 0, &a.pubkey(), &tok, &TOKEN)], &[&w.payer]));
    assert_eq!(token_balance(&w.svm, &ata(&a.pubkey(), &tok, &TOKEN)), 495);
    ok(send(&mut w.svm, &[close_receipt(7, &booster.pubkey(), &tok), close_token_asset(7, &tok, &TOKEN, &booster.pubkey()), close_pool(7)], &[&w.payer]));
    // ...but a new pool cannot start while Suspended.
    assert_err(&send(&mut w.svm, &[start_match(8)], &[&w.payer]), 6001);
}

/// Stray tokens sent straight into a vault are burned on close (never block it).
#[test]
fn stray_tokens_are_burned_on_close() {
    let mut w = world();
    let a = Keypair::new();
    let booster = funded(&mut w.svm);
    let tok = fungible(&mut w.svm, &w.payer, &booster.pubkey(), &TOKEN, 1_000);
    create_ata(&mut w.svm, &w.payer, &a.pubkey(), &tok, &TOKEN);
    ok(send(&mut w.svm, &[start_match(9)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(9, &booster.pubkey(), &tok, &TOKEN, 100, "")], &[&booster]));
    let vault = vault_pda(&asset_pda(&pool_pda(9), &tok));
    mint_to(&mut w.svm, &w.payer, &tok, &vault, &TOKEN, 7); // stray
    assert_eq!(outcome(send(&mut w.svm, &[finish(9, vec![Some(a.pubkey())])], &[&w.payer])), 0);
    ok(send(&mut w.svm, &[collect_fee_token(9, &tok, &TOKEN), claim_token(9, 0, &a.pubkey(), &tok, &TOKEN)], &[&w.payer]));
    assert_eq!(token_balance(&w.svm, &ata(&a.pubkey(), &tok, &TOKEN)), 99, "stray not paid out");
    assert_eq!(token_balance(&w.svm, &vault), 7);
    let supply = |svm: &litesvm::LiteSVM| anchor_spl::token_2022::spl_token_2022::extension::StateWithExtensions::<anchor_spl::token_2022::spl_token_2022::state::Mint>::unpack(&svm.get_account(&tok).unwrap().data).unwrap().base.supply;
    let s0 = supply(&w.svm);
    ok(send(&mut w.svm, &[close_receipt(9, &booster.pubkey(), &tok), close_token_asset(9, &tok, &TOKEN, &booster.pubkey()), close_pool(9)], &[&w.payer]));
    assert!(gone(&w.svm, &vault));
    assert_eq!(s0 - supply(&w.svm), 7, "stray burned (supply dropped), not silently lost");
}

/// Token-2022 Pausable mint: vault gets PausableAccount (Anchor init sizing).
#[test]
fn pausable_token_2022_mint_can_be_boosted() {
    let mut w = world();
    let booster = funded(&mut w.svm);
    let mint = create_mint(&mut w.svm, &w.payer, &TOKEN_2022, 6, &[anchor_spl::token_2022::spl_token_2022::extension::ExtensionType::Pausable]);
    allow_mint(&mut w.svm, &w.payer, &mint, &TOKEN_2022, 6, true);
    let b_ata = create_ata(&mut w.svm, &w.payer, &booster.pubkey(), &mint, &TOKEN_2022);
    mint_to(&mut w.svm, &w.payer, &mint, &b_ata, &TOKEN_2022, 1_000);
    ok(send(&mut w.svm, &[start_match(10)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(10, &booster.pubkey(), &mint, &TOKEN_2022, 1_000, "")], &[&booster]));
    let vault = w.svm.get_account(&vault_pda(&asset_pda(&pool_pda(10), &mint))).unwrap();
    assert_eq!(vault.data.len(), 170, "165 + PausableAccount");
}

/// No ATA → claim_token fails (rights persist) → claim_token_with_ata with payer = winner.
#[test]
fn claim_with_ata_when_winner_has_no_ata() {
    let mut w = world();
    let a = funded(&mut w.svm);
    let booster = funded(&mut w.svm);
    let tok = fungible(&mut w.svm, &w.payer, &booster.pubkey(), &TOKEN, 1_000);
    ok(send(&mut w.svm, &[start_match(11)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(11, &booster.pubkey(), &tok, &TOKEN, 1_000, "")], &[&booster]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(11, vec![Some(a.pubkey())])], &[&w.payer])), 0);
    assert!(send(&mut w.svm, &[claim_token(11, 0, &a.pubkey(), &tok, &TOKEN)], &[&w.payer]).is_err(), "no ATA yet");
    // payer == winner (same key in a mut Signer and a read-only field)
    ok(send(&mut w.svm, &[claim_token_with_ata(11, 0, &a.pubkey(), &a.pubkey(), &tok, &TOKEN)], &[&a]));
    assert_eq!(token_balance(&w.svm, &ata(&a.pubkey(), &tok, &TOKEN)), 990);
}

/// Relayer shape: the client marks `winner` as a signer — the program does not care.
#[test]
fn claim_sol_with_winner_marked_signer() {
    let mut w = world();
    let a = funded(&mut w.svm);
    ok(send(&mut w.svm, &[start_match_sol(12, vec![1_000_000])], &[&w.payer]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(12, vec![Some(a.pubkey())])], &[&w.payer])), 0);
    let mut ix = claim_sol(12, 0, &a.pubkey());
    ix.accounts.iter_mut().find(|m| m.pubkey == a.pubkey()).unwrap().is_signer = true;
    let before = lamports(&w.svm, &a.pubkey());
    let meta = ok(send(&mut w.svm, &[ix], &[&w.payer, &a]));
    println!("claim_sol: {} CU", meta.compute_units_consumed);
    assert_eq!(lamports(&w.svm, &a.pubkey()) - before, 1_000_000);
}
