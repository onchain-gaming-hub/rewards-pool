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
    ok(send(&mut w.svm, &[refund_sol(1, &b1.pubkey())], &[&w.payer]));
    assert!(lamports(&w.svm, &b1.pubkey()) - before > SOL / 10 + SOL / 20, "both boosts + receipt rent back");
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
