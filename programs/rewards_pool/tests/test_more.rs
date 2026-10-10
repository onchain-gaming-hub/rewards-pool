//! Paths added after dev-qa code review round 1.
mod common;
use {
    anchor_lang::{prelude::Pubkey, solana_program::system_instruction, InstructionData, ToAccountMetas},
    anchor_spl::token_2022::spl_token_2022::instruction as t22_ix,
    common::*,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

#[test]
fn partial_refund_blocks_close_until_every_booster_is_refunded() {
    let mut w = world();
    let (b1, b2) = (funded(&mut w.svm), funded(&mut w.svm));
    ok(send(&mut w.svm, &[start_match(1)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_sol(1, &b1.pubkey(), SOL / 10, "")], &[&b1]));
    ok(send(&mut w.svm, &[boost_sol(1, &b1.pubkey(), SOL / 20, "again")], &[&b1])); // same booster twice → one receipt
    ok(send(&mut w.svm, &[boost_sol(1, &b2.pubkey(), SOL / 10, "")], &[&b2]));
    assert_eq!(pool(&w.svm, 1).open_receipts, 2);
    assert_err(&send(&mut w.svm, &[refund_sol(1, &b1.pubkey())], &[&w.payer]), 6021); // InvalidStatus (Open)
    assert_err(&send(&mut w.svm, &[close_sol_asset(1, &b1.pubkey())], &[&w.payer]), 6021);
    assert_eq!(outcome(send(&mut w.svm, &[cancel_match(1)], &[&w.payer])), 20);
    assert_err(&send(&mut w.svm, &[close_receipt(1, &b1.pubkey(), &Pubkey::default())], &[&w.payer]), 6012); // PoolNotSettled
    let before = lamports(&w.svm, &b1.pubkey());
    let receipt_rent = lamports(&w.svm, &receipt_pda(&pool_pda(1), &b1.pubkey(), &Pubkey::default()));
    ok(send(&mut w.svm, &[refund_sol(1, &b1.pubkey())], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &b1.pubkey()) - before, SOL / 10 + SOL / 20 + receipt_rent, "both boosts + receipt rent back");
    assert_err(&send(&mut w.svm, &[close_sol_asset(1, &b1.pubkey())], &[&w.payer]), 6022); // b2 not refunded yet
    ok(send(&mut w.svm, &[refund_sol(1, &b2.pubkey()), close_sol_asset(1, &b1.pubkey()), close_pool(1)], &[&w.payer]));
}

#[test]
fn token_refunds_including_with_ata_and_token_base_return_on_cancel() {
    let mut w = world();
    let booster = funded(&mut w.svm);
    let point = fungible(&mut w.svm, &w.payer, &game_authority(), &TOKEN, 1_000);
    let tok = create_mint(&mut w.svm, &w.payer, &TOKEN, 6, &[]);
    allow_mint(&mut w.svm, &w.payer, &tok, &TOKEN, 6, true);
    // Booster boosts from a NON-ATA token account → no ATA exists for the refund.
    let src = Keypair::new();
    let rent = w.svm.minimum_balance_for_rent_exemption(TOKEN_ACCOUNT_LEN);
    ok(send(&mut w.svm, &[
        system_instruction::create_account(&w.payer.pubkey(), &src.pubkey(), rent, TOKEN_ACCOUNT_LEN as u64, &TOKEN),
        t22_ix::initialize_account3(&TOKEN, &src.pubkey(), &tok, &booster.pubkey()).unwrap(),
        t22_ix::mint_to(&TOKEN, &tok, &src.pubkey(), &w.payer.pubkey(), &[], 500).unwrap(),
    ], &[&w.payer, &src]));
    ok(send(&mut w.svm, &[start_match_token(2, &point, &TOKEN, vec![100, 50])], &[&w.payer]));
    let mut ix = boost_token(2, &booster.pubkey(), &tok, &TOKEN, 500, "");
    ix.accounts[4].pubkey = src.pubkey(); // booster_token
    ok(send(&mut w.svm, &[ix], &[&booster]));
    assert_eq!(outcome(send(&mut w.svm, &[cancel_match(2)], &[&w.payer])), 20);

    assert!(send(&mut w.svm, &[refund_token(2, &booster.pubkey(), &tok, &TOKEN)], &[&w.payer]).is_err(), "booster has no ATA");
    let p = pool_pda(2);
    let a = asset_pda(&p, &tok);
    let with_ata = rix(
        rewards_pool::accounts::RefundBoostTokenWithAta {
            payer: w.payer.pubkey(),
            match_pool: p,
            pool_asset: a,
            mint: tok,
            vault: vault_pda(&a),
            boost_receipt: receipt_pda(&p, &booster.pubkey(), &tok),
            booster: booster.pubkey(),
            booster_ata: ata(&booster.pubkey(), &tok, &TOKEN),
            token_program: TOKEN,
            associated_token_program: ATA_PROGRAM,
            system_program: anchor_lang::solana_program::system_program::ID,
        }
        .to_account_metas(None),
        rewards_pool::instruction::RefundBoostTokenWithAta {}.data(),
    );
    ok(send(&mut w.svm, &[with_ata], &[&w.payer])); // crank pays the ATA rent
    assert_eq!(token_balance(&w.svm, &ata(&booster.pubkey(), &tok, &TOKEN)), 500);

    let ga_ata = ata(&game_authority(), &point, &TOKEN);
    let before = token_balance(&w.svm, &ga_ata);
    ok(send(&mut w.svm, &[return_base_token(2, &point, &TOKEN)], &[&w.payer]));
    assert_eq!(token_balance(&w.svm, &ga_ata) - before, 150, "full base back on cancel");
    ok(send(&mut w.svm, &[close_token_asset(2, &tok, &TOKEN, &booster.pubkey()), close_token_asset(2, &point, &TOKEN, &game_authority()), close_pool(2)], &[&w.payer]));
}

#[test]
fn receipt_of_another_asset_and_token_program_mismatch_are_rejected() {
    let mut w = world();
    let booster = funded(&mut w.svm);
    let tok = fungible(&mut w.svm, &w.payer, &booster.pubkey(), &TOKEN, 100);
    ok(send(&mut w.svm, &[start_match(3)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_sol(3, &booster.pubkey(), SOL / 100, ""), boost_token(3, &booster.pubkey(), &tok, &TOKEN, 10, "")], &[&booster]));
    assert_eq!(outcome(send(&mut w.svm, &[cancel_match(3)], &[&w.payer])), 20);
    let mut ix = refund_token(3, &booster.pubkey(), &tok, &TOKEN);
    ix.accounts[4].pubkey = receipt_pda(&pool_pda(3), &booster.pubkey(), &Pubkey::default()); // SOL receipt
    assert_err(&send(&mut w.svm, &[ix], &[&w.payer]), 6029); // ReceiptAssetMismatch

    // AllowedMint says Token-2022 but the mint is SPL Token.
    let m = create_mint(&mut w.svm, &w.payer, &TOKEN, 6, &[]);
    allow_mint(&mut w.svm, &w.payer, &m, &anchor_spl::token_2022::ID, 6, false);
    let b_ata = create_ata(&mut w.svm, &w.payer, &booster.pubkey(), &m, &TOKEN);
    mint_to(&mut w.svm, &w.payer, &m, &b_ata, &TOKEN, 10);
    ok(send(&mut w.svm, &[start_match(4)], &[&w.payer]));
    assert_err(&send(&mut w.svm, &[boost_token(4, &booster.pubkey(), &m, &TOKEN, 10, "")], &[&booster]), 6011);
}

#[test]
fn winners_are_truncated_to_32_and_duplicate_pubkeys_claim_by_index() {
    let mut w = world();
    let agent = funded(&mut w.svm); // W14: one wallet in several slots
    ok(send(&mut w.svm, &[start_match_sol(5, vec![1_000_000, 1_000_000, 1_000_000])], &[&w.payer]));
    let mut winners = vec![Some(agent.pubkey()); 3];
    winners.extend((0..37).map(|_| Some(Pubkey::new_unique())));
    assert_eq!(outcome(send(&mut w.svm, &[finish(5, winners)], &[&w.payer])), 0);
    assert_eq!(pool(&w.svm, 5).winners.len(), 32);
    let before = lamports(&w.svm, &agent.pubkey());
    ok(send(&mut w.svm, &[claim_sol(5, 0, &agent.pubkey()), claim_sol(5, 1, &agent.pubkey()), claim_sol(5, 2, &agent.pubkey())], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &agent.pubkey()) - before, 3_000_000);
}

#[test]
fn booster_gets_asset_and_vault_rent_back_on_close() {
    let mut w = world();
    let (a, booster) = (Keypair::new(), funded(&mut w.svm));
    let tok = fungible(&mut w.svm, &w.payer, &booster.pubkey(), &TOKEN, 100);
    create_ata(&mut w.svm, &w.payer, &a.pubkey(), &tok, &TOKEN);
    ok(send(&mut w.svm, &[start_match(6)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(6, &booster.pubkey(), &tok, &TOKEN, 100, "")], &[&booster]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(6, vec![Some(a.pubkey())])], &[&w.payer])), 0);
    ok(send(&mut w.svm, &[collect_fee_token(6, &tok, &TOKEN), claim_token(6, 0, &a.pubkey(), &tok, &TOKEN), close_receipt(6, &booster.pubkey(), &tok)], &[&w.payer]));
    let pa = asset_pda(&pool_pda(6), &tok);
    let expected = lamports(&w.svm, &pa) + lamports(&w.svm, &vault_pda(&pa));
    let before = lamports(&w.svm, &booster.pubkey());
    ok(send(&mut w.svm, &[close_token_asset(6, &tok, &TOKEN, &booster.pubkey())], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &booster.pubkey()) - before, expected, "PoolAsset + vault rent back to the booster");
}

#[test]
fn fee_above_cap_is_rejected_at_open() {
    let mut w = world();
    put_config(&mut w.svm, 2_001);
    assert_err(&send(&mut w.svm, &[start_match(7)], &[&w.payer]), 6034); // FeeTooHigh
}

// ───────── Fable code review round 1 additions ─────────

#[test]
fn prefunded_pdas_do_not_block_init() {
    let mut w = world();
    let booster = funded(&mut w.svm);
    let min = w.svm.minimum_balance_for_rent_exemption(0);
    let p = pool_pda(8);
    for addr in [p, asset_pda(&p, &Pubkey::default()), receipt_pda(&p, &booster.pubkey(), &Pubkey::default())] {
        ok(send(&mut w.svm, &[system_instruction::transfer(&w.payer.pubkey(), &addr, min)], &[&w.payer]));
    }
    let a = Keypair::new();
    ok(send(&mut w.svm, &[start_match_sol(8, vec![1_000_000])], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_sol(8, &booster.pubkey(), SOL / 100, "")], &[&booster]));
    let st = asset(&w.svm, &asset_pda(&p, &Pubkey::default()));
    assert_eq!((st.base_amount, st.boost_received), (1_000_000, SOL / 100), "pre-funded lamports are not booked");
    assert_eq!(outcome(send(&mut w.svm, &[finish(8, vec![Some(a.pubkey())])], &[&w.payer])), 0);
    ok(send(&mut w.svm, &[collect_fee_sol(8), claim_sol(8, 0, &a.pubkey()), close_receipt(8, &booster.pubkey(), &Pubkey::default())], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &a.pubkey()), 1_000_000 + SOL / 100 - SOL / 10_000, "winner gets exactly base + boost − fee");
    // The pre-funded stray lamports on the PoolAsset go to its rent_payer (game_authority) on close.
    let pa = asset_pda(&p, &Pubkey::default());
    let left = lamports(&w.svm, &pa);
    let ga0 = lamports(&w.svm, &game_authority());
    ok(send(&mut w.svm, &[close_sol_asset(8, &game_authority())], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &game_authority()) - ga0, left, "rent + stray to rent_payer");
}

#[test]
fn crank_pays_ata_for_winner_and_same_booster_token_twice_accumulates() {
    let mut w = world();
    let (a, booster, crank) = (Keypair::new(), funded(&mut w.svm), funded(&mut w.svm));
    let tok = fungible(&mut w.svm, &w.payer, &booster.pubkey(), &TOKEN, 1_000);
    ok(send(&mut w.svm, &[start_match(9)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(9, &booster.pubkey(), &tok, &TOKEN, 300, ""), boost_token(9, &booster.pubkey(), &tok, &TOKEN, 200, "")], &[&booster]));
    assert_eq!(pool(&w.svm, 9).open_receipts, 1);
    assert_eq!(asset(&w.svm, &asset_pda(&pool_pda(9), &tok)).boost_received, 500);
    assert_eq!(outcome(send(&mut w.svm, &[finish(9, vec![Some(a.pubkey())])], &[&w.payer])), 0);
    ok(send(&mut w.svm, &[claim_token_with_ata(9, 0, &crank.pubkey(), &a.pubkey(), &tok, &TOKEN)], &[&crank]));
    assert_eq!(token_balance(&w.svm, &ata(&a.pubkey(), &tok, &TOKEN)), 495);
}

#[test]
fn nft_without_treasury_ata_cannot_be_fee_collected_and_fee_zero_is_fine() {
    let mut w = world();
    put_config(&mut w.svm, 0); // fee 0%
    let (a, booster) = (funded(&mut w.svm), funded(&mut w.svm));
    let n = nft(&mut w.svm, &w.payer, &booster.pubkey()); // pure 1/1 → no Treasury ATA
    ok(send(&mut w.svm, &[start_match(10)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(10, &booster.pubkey(), &n, &TOKEN, 1, ""), boost_sol(10, &booster.pubkey(), SOL / 100, "")], &[&booster]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(10, vec![Some(a.pubkey())])], &[&w.payer])), 0);
    assert_err(&send(&mut w.svm, &[collect_fee_token(10, &n, &TOKEN)], &[&w.payer]), 3012); // spec §5.6
    let t0 = lamports(&w.svm, &treasury_pda().0);
    ok(send(&mut w.svm, &[collect_fee_sol(10)], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &treasury_pda().0), t0, "fee 0 → flag only");
    ok(send(&mut w.svm, &[claim_sol(10, 0, &a.pubkey()), claim_token_with_ata(10, 0, &a.pubkey(), &a.pubkey(), &n, &TOKEN)], &[&a]));
    ok(send(&mut w.svm, &[close_receipt(10, &booster.pubkey(), &n), close_receipt(10, &booster.pubkey(), &Pubkey::default()), close_token_asset(10, &n, &TOKEN, &booster.pubkey()), close_sol_asset(10, &booster.pubkey()), close_pool(10)], &[&w.payer]));
}

#[test]
fn cancel_and_expire_from_boosts_locked() {
    let mut w = world();
    set_time(&mut w.svm, 5_000_000);
    ok(send(&mut w.svm, &[start_match(11), start_match(12)], &[&w.payer]));
    assert_eq!(outcome(send(&mut w.svm, &[lock(11)], &[&w.payer])), 0);
    assert_eq!(outcome(send(&mut w.svm, &[cancel_match(11)], &[&w.payer])), 20);
    assert_eq!(outcome(send(&mut w.svm, &[lock(12)], &[&w.payer])), 0);
    set_time(&mut w.svm, 5_000_000 + 7 * 86_400);
    ok(send(&mut w.svm, &[expire(12)], &[&w.payer]));
    assert_eq!(pool(&w.svm, 12).status, rewards_pool::state::PoolStatus::Cancelled);
}

#[test]
fn wrong_token_program_on_claim_is_rejected() {
    let mut w = world();
    let (a, booster) = (Keypair::new(), funded(&mut w.svm));
    let tok = fungible(&mut w.svm, &w.payer, &booster.pubkey(), &TOKEN, 100);
    create_ata(&mut w.svm, &w.payer, &a.pubkey(), &tok, &TOKEN);
    ok(send(&mut w.svm, &[start_match(13)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(13, &booster.pubkey(), &tok, &TOKEN, 100, "")], &[&booster]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(13, vec![Some(a.pubkey())])], &[&w.payer])), 0);
    let mut ix = claim_token(13, 0, &a.pubkey(), &tok, &TOKEN);
    ix.accounts.last_mut().unwrap().pubkey = anchor_spl::token_2022::ID;
    let r = send(&mut w.svm, &[ix], &[&w.payer]);
    let e = format!("{:?}", r.as_ref().expect_err("must fail").err);
    println!("wrong token program on claim → {e}");
    assert!(e.contains("Custom(2022)"), "Anchor ConstraintMintTokenProgram fires first: {e}");
}

#[test]
fn token_base_returns_to_a_recreated_game_ata() {
    let mut w = world();
    let point = fungible(&mut w.svm, &w.payer, &game_authority(), &TOKEN, 1_000);
    ok(send(&mut w.svm, &[start_match_token(14, &point, &TOKEN, vec![100])], &[&w.payer]));
    assert_eq!(outcome(send(&mut w.svm, &[cancel_match(14)], &[&w.payer])), 20);
    // Simulate the game closing its ATA, then anyone re-creating it at the same address.
    let ga_ata = ata(&game_authority(), &point, &TOKEN);
    w.svm.set_account(ga_ata, solana_account::Account::default()).unwrap();
    create_ata(&mut w.svm, &w.payer, &game_authority(), &point, &TOKEN);
    ok(send(&mut w.svm, &[return_base_token(14, &point, &TOKEN)], &[&w.payer]));
    assert_eq!(token_balance(&w.svm, &ga_ata), 100);
}

#[test]
fn compute_units_of_main_paths() {
    let mut w = world();
    let (a, booster) = (Keypair::new(), funded(&mut w.svm));
    let tok = fungible(&mut w.svm, &w.payer, &booster.pubkey(), &TOKEN, 1_000);
    create_ata(&mut w.svm, &w.payer, &a.pubkey(), &tok, &TOKEN);
    let cu = |m: litesvm::types::TransactionMetadata| m.compute_units_consumed;
    let open = cu(ok(send(&mut w.svm, &[start_match_sol(15, vec![1_000_000])], &[&w.payer])));
    let bs = cu(ok(send(&mut w.svm, &[boost_sol(15, &booster.pubkey(), SOL / 100, "")], &[&booster])));
    let bt = cu(ok(send(&mut w.svm, &[boost_token(15, &booster.pubkey(), &tok, &TOKEN, 100, "")], &[&booster])));
    let st = cu(ok(send(&mut w.svm, &[finish(15, vec![Some(a.pubkey())])], &[&w.payer])));
    let fee = cu(ok(send(&mut w.svm, &[collect_fee_token(15, &tok, &TOKEN)], &[&w.payer])));
    let cs = cu(ok(send(&mut w.svm, &[claim_sol(15, 0, &a.pubkey())], &[&w.payer])));
    let ct = cu(ok(send(&mut w.svm, &[claim_token(15, 0, &a.pubkey(), &tok, &TOKEN)], &[&w.payer])));
    println!("CU · open_pool_sol(via game) {open} · boost_sol {bs} · boost_token {bt} · settle(via game) {st} · collect_fee_token {fee} · claim_sol {cs} · claim_token {ct}");
    assert!(ct < 50_000 && cs < 50_000);
}

#[test]
fn same_mint_base_and_boost_without_rank_one_settles_cleanly() {
    let mut w = world();
    let (b, booster) = (funded(&mut w.svm), funded(&mut w.svm));
    let point = fungible(&mut w.svm, &w.payer, &game_authority(), &TOKEN, 1_000);
    let b_ata = create_ata(&mut w.svm, &w.payer, &booster.pubkey(), &point, &TOKEN);
    mint_to(&mut w.svm, &w.payer, &point, &b_ata, &TOKEN, 500);
    create_ata(&mut w.svm, &w.payer, &b.pubkey(), &point, &TOKEN);
    ok(send(&mut w.svm, &[start_match_token(16, &point, &TOKEN, vec![100, 40])], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(16, &booster.pubkey(), &point, &TOKEN, 500, "")], &[&booster]));
    let vault = vault_pda(&asset_pda(&pool_pda(16), &point));
    mint_to(&mut w.svm, &w.payer, &point, &vault, &TOKEN, 3); // stray
    assert_eq!(outcome(send(&mut w.svm, &[finish(16, vec![None, Some(b.pubkey())])], &[&w.payer])), 0);
    let ga_ata = ata(&game_authority(), &point, &TOKEN);
    let g0 = token_balance(&w.svm, &ga_ata);
    ok(send(&mut w.svm, &[claim_token(16, 1, &b.pubkey(), &point, &TOKEN), refund_token(16, &booster.pubkey(), &point, &TOKEN), return_base_token(16, &point, &TOKEN)], &[&w.payer]));
    assert_eq!(token_balance(&w.svm, &ata(&b.pubkey(), &point, &TOKEN)), 40);
    assert_eq!(token_balance(&w.svm, &b_ata), 500, "booster refunded in full, no fee");
    assert_eq!(token_balance(&w.svm, &ga_ata) - g0, 100, "rank-1 base back to the game");
    assert_eq!(token_balance(&w.svm, &vault), 3, "only the stray is left");
    ok(send(&mut w.svm, &[close_token_asset(16, &point, &TOKEN, &game_authority()), close_pool(16)], &[&w.payer]));
    assert!(gone(&w.svm, &vault) && gone(&w.svm, &pool_pda(16)));
}

// ───────── independent review round 2 additions ─────────

#[test]
fn claim_sol_to_a_program_owned_wallet() {
    let mut w = world();
    let squads_like = Pubkey::new_unique();
    let rent = w.svm.minimum_balance_for_rent_exemption(0);
    w.svm.set_account(squads_like, solana_account::Account { lamports: rent, data: vec![], owner: Pubkey::new_unique(), executable: false, rent_epoch: 0 }).unwrap();
    ok(send(&mut w.svm, &[start_match_sol(17, vec![1_000_000])], &[&w.payer]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(17, vec![Some(squads_like)])], &[&w.payer])), 0);
    ok(send(&mut w.svm, &[claim_sol(17, 0, &squads_like)], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &squads_like), rent + 1_000_000, "UncheckedAccount destination works for program-owned wallets");
}

#[test]
fn pausable_vault_closes_after_flow_with_stray() {
    let mut w = world();
    let (a, booster) = (Keypair::new(), funded(&mut w.svm));
    let mint = create_mint(&mut w.svm, &w.payer, &anchor_spl::token_2022::ID, 6, &[anchor_spl::token_2022::spl_token_2022::extension::ExtensionType::Pausable]);
    allow_mint(&mut w.svm, &w.payer, &mint, &anchor_spl::token_2022::ID, 6, true);
    let t22 = anchor_spl::token_2022::ID;
    let b_ata = create_ata(&mut w.svm, &w.payer, &booster.pubkey(), &mint, &t22);
    mint_to(&mut w.svm, &w.payer, &mint, &b_ata, &t22, 1_000);
    create_ata(&mut w.svm, &w.payer, &a.pubkey(), &mint, &t22);
    ok(send(&mut w.svm, &[start_match(18)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(18, &booster.pubkey(), &mint, &t22, 1_000, "")], &[&booster]));
    let vault = vault_pda(&asset_pda(&pool_pda(18), &mint));
    mint_to(&mut w.svm, &w.payer, &mint, &vault, &t22, 5); // stray
    assert_eq!(outcome(send(&mut w.svm, &[finish(18, vec![Some(a.pubkey())])], &[&w.payer])), 0);
    ok(send(&mut w.svm, &[collect_fee_token(18, &mint, &t22), claim_token(18, 0, &a.pubkey(), &mint, &t22), close_receipt(18, &booster.pubkey(), &mint)], &[&w.payer]));
    ok(send(&mut w.svm, &[close_token_asset(18, &mint, &t22, &booster.pubkey()), close_pool(18)], &[&w.payer]));
    assert!(gone(&w.svm, &vault), "170-byte PausableAccount vault burns stray and closes");
}

#[test]
fn expire_refund_close_then_game_settle_is_noop_closed() {
    let mut w = world();
    let booster = funded(&mut w.svm);
    set_time(&mut w.svm, 9_000_000);
    ok(send(&mut w.svm, &[start_match(19)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_sol(19, &booster.pubkey(), SOL / 100, "")], &[&booster]));
    set_time(&mut w.svm, 9_000_000 + 7 * 86_400);
    ok(send(&mut w.svm, &[expire(19), refund_sol(19, &booster.pubkey()), close_sol_asset(19, &booster.pubkey()), close_pool(19)], &[&w.payer]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(19, vec![Some(booster.pubkey())])], &[&w.payer])), 13, "NoOp(Closed) — never errors in the game's tx");
}

#[test]
fn unpayable_winner_keys_are_treated_as_none() {
    let mut w = world();
    let c = Keypair::new();
    ok(send(&mut w.svm, &[start_match_sol(20, vec![1_000_000, 1_000_000, 1_000_000])], &[&w.payer]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(20, vec![Some(Pubkey::default()), Some(pool_pda(20)), Some(c.pubkey())])], &[&w.payer])), 0);
    assert_eq!(pool(&w.svm, 20).winners, vec![None, None, Some(c.pubkey())]);
    let g0 = lamports(&w.svm, &game_authority());
    ok(send(&mut w.svm, &[claim_sol(20, 2, &c.pubkey()), return_base_sol(20)], &[&w.payer]));
    assert_eq!(lamports(&w.svm, &game_authority()) - g0, 2_000_000, "two unpayable slots → base back to the game");
    ok(send(&mut w.svm, &[close_sol_asset(20, &game_authority()), close_pool(20)], &[&w.payer]));
    // Only unpayable keys → R13 cancel.
    ok(send(&mut w.svm, &[start_match(21)], &[&w.payer]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(21, vec![Some(Pubkey::default())])], &[&w.payer])), 21);
}

#[test]
fn claim_before_collect_fee_gives_the_same_result() {
    let mut w = world();
    let (a, booster) = (Keypair::new(), funded(&mut w.svm));
    let tok = fungible(&mut w.svm, &w.payer, &booster.pubkey(), &TOKEN, 1_000);
    create_ata(&mut w.svm, &w.payer, &a.pubkey(), &tok, &TOKEN);
    ok(send(&mut w.svm, &[start_match(22)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(22, &booster.pubkey(), &tok, &TOKEN, 1_000, "")], &[&booster]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(22, vec![Some(a.pubkey())])], &[&w.payer])), 0);
    ok(send(&mut w.svm, &[claim_token(22, 0, &a.pubkey(), &tok, &TOKEN)], &[&w.payer]));
    assert_eq!(token_balance(&w.svm, &ata(&a.pubkey(), &tok, &TOKEN)), 990);
    assert_err(&send(&mut w.svm, &[close_token_asset(22, &tok, &TOKEN, &booster.pubkey())], &[&w.payer]), 6022); // fee still owed
    ok(send(&mut w.svm, &[collect_fee_token(22, &tok, &TOKEN)], &[&w.payer]));
    assert_eq!(token_balance(&w.svm, &ata(&treasury_pda().0, &tok, &TOKEN)), 10);
    ok(send(&mut w.svm, &[close_receipt(22, &booster.pubkey(), &tok), close_token_asset(22, &tok, &TOKEN, &booster.pubkey()), close_pool(22)], &[&w.payer]));
}
