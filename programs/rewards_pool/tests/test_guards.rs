//! Negative paths: each spec-v2 §10 error fires where the spec says it does.
mod common;
use {
    anchor_lang::{
        prelude::Pubkey,
        solana_program::{instruction::{AccountMeta, Instruction}, system_program},
        InstructionData, ToAccountMetas,
    },
    anchor_spl::token_2022::spl_token_2022::{extension::StateWithExtensions, state::Mint as MintState},
    common::*,
    rewards_pool::state::AssetKind,
    solana_keypair::Keypair,
    solana_signer::Signer,
};

#[test]
fn open_pool_guards() {
    let mut w = world();
    let rent0 = w.svm.minimum_balance_for_rent_exemption(0);
    assert_err(&send(&mut w.svm, &[start_match_sol(1, vec![rent0 - 1])], &[&w.payer]), 6027); // PayoutBelowRentExempt (F9)
    assert_err(&send(&mut w.svm, &[start_match_sol(1, vec![])], &[&w.payer]), 6026); // BaseAccountsMismatch (§10)
    assert_err(&send(&mut w.svm, &[start_match_sol(1, vec![0])], &[&w.payer]), 6003);
    assert_err(&send(&mut w.svm, &[start_match_sol(1, vec![1_000_000; 33])], &[&w.payer]), 6002); // TooManyRanks

    // Direct call without the game's PDA signature.
    let attacker = funded(&mut w.svm);
    let ix = rix(
        rewards_pool::accounts::OpenPool {
            game_authority: attacker.pubkey(),
            game_program: GAME,
            game_entry: game_entry_pda(),
            config: config_pda(),
            match_pool: pool_pda(1),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        rewards_pool::instruction::OpenPool { room_id: 1 }.data(),
    );
    assert_err(&send(&mut w.svm, &[ix], &[&attacker]), 6000); // NotGameAuthority

    // Token base: not allowlisted → AllowedMint missing (Anchor 3012) · NFT base → 6025.
    let unlisted = create_mint(&mut w.svm, &w.payer, &TOKEN, 6, &[]);
    let src = create_ata(&mut w.svm, &w.payer, &game_authority(), &unlisted, &TOKEN);
    mint_to(&mut w.svm, &w.payer, &unlisted, &src, &TOKEN, 100);
    assert_err(&send(&mut w.svm, &[start_match_token(2, &unlisted, &TOKEN, vec![10])], &[&w.payer]), 3012);
    let n = nft(&mut w.svm, &w.payer, &game_authority());
    assert_err(&send(&mut w.svm, &[start_match_token(2, &n, &TOKEN, vec![1])], &[&w.payer]), 6025); // BaseMustBeFungible
}

#[test]
fn boost_guards() {
    let mut w = world();
    let b = funded(&mut w.svm);
    ok(send(&mut w.svm, &[start_match(1)], &[&w.payer]));
    assert_err(&send(&mut w.svm, &[boost_sol(1, &b.pubkey(), 999_999, "")], &[&b]), 6008); // BoostTooSmall
    assert_err(&send(&mut w.svm, &[boost_sol(1, &b.pubkey(), 1_000_000, &"ก".repeat(47))], &[&b]), 6007); // 141 bytes
    ok(send(&mut w.svm, &[boost_sol(1, &b.pubkey(), 1_000_000, &"ก".repeat(46))], &[&b])); // 138 bytes OK

    let n = nft(&mut w.svm, &w.payer, &b.pubkey());
    assert_err(&send(&mut w.svm, &[boost_token(1, &b.pubkey(), &n, &TOKEN, 2, "")], &[&b]), 6010); // InvalidNftAmount

    // 8 boosted assets max: SOL already took one slot → 7 more tokens, the 8th token fails.
    for i in 0..8 {
        let m = fungible(&mut w.svm, &w.payer, &b.pubkey(), &TOKEN, 10);
        let r = send(&mut w.svm, &[boost_token(1, &b.pubkey(), &m, &TOKEN, 1, "")], &[&b]);
        if i < 7 { ok(r); } else { assert_err(&r, 6006); } // TooManyAssets
    }

    assert_eq!(outcome(send(&mut w.svm, &[lock(1)], &[&w.payer])), 0);
    assert_eq!(outcome(send(&mut w.svm, &[lock(1)], &[&w.payer])), 10, "NoOp(AlreadyLocked)");
    assert_err(&send(&mut w.svm, &[boost_sol(1, &b.pubkey(), 1_000_000, "")], &[&b]), 6005); // PoolNotOpen
}

#[test]
fn boost_sol_net_of_fee_must_stay_rent_exempt() {
    let mut w = world();
    put_config(&mut w.svm, 2_000); // 20% (C2 ceiling)
    let b = funded(&mut w.svm);
    ok(send(&mut w.svm, &[start_match(1)], &[&w.payer]));
    // 1_000_000 - 20% = 800_000 < LiteSVM rent-exempt (890_880)
    assert_err(&send(&mut w.svm, &[boost_sol(1, &b.pubkey(), 1_000_000, "")], &[&b]), 6031);
    ok(send(&mut w.svm, &[boost_sol(1, &b.pubkey(), 1_200_000, "")], &[&b]));
}

#[test]
fn game_call_guards() {
    let mut w = world();
    ok(send(&mut w.svm, &[start_match(1), start_match(2)], &[&w.payer]));
    let p = Keypair::new().pubkey();
    // Wrong PDA for the room → WrongMatchPool, never a silent NoOp.
    assert_err(&send(&mut w.svm, &[finish_with_pool(1, pool_pda(2), vec![Some(p)])], &[&w.payer]), 6033);
    // Direct settle without the game PDA signature.
    let attacker = funded(&mut w.svm);
    let ix = rix(
        rewards_pool::accounts::GameCall { game_authority: attacker.pubkey(), game_program: GAME, match_pool: pool_pda(1) }.to_account_metas(None),
        rewards_pool::instruction::Settle { room_id: 1, winners: vec![Some(attacker.pubkey())] }.data(),
    );
    assert_err(&send(&mut w.svm, &[ix], &[&attacker]), 6000);
    assert_eq!(outcome(send(&mut w.svm, &[finish(1, vec![Some(p)])], &[&w.payer])), 0);
    assert_eq!(outcome(send(&mut w.svm, &[finish(1, vec![Some(p)])], &[&w.payer])), 11, "NoOp(AlreadySettled)");
    assert_eq!(outcome(send(&mut w.svm, &[cancel_match(1)], &[&w.payer])), 11, "never cancel after settle");
    assert_err(&send(&mut w.svm, &[expire(1)], &[&w.payer]), 6021); // InvalidStatus
    // A room that never existed → NoOp(Closed).
    assert_eq!(outcome(send(&mut w.svm, &[finish(77, vec![Some(p)])], &[&w.payer])), 13);
}

#[test]
fn claim_guards() {
    let mut w = world();
    let (a, b, d) = (funded(&mut w.svm), funded(&mut w.svm), funded(&mut w.svm));
    ok(send(&mut w.svm, &[start_match_sol(1, vec![2_000_000, 1_000_000, 1_000_000])], &[&w.payer]));
    assert_err(&send(&mut w.svm, &[claim_sol(1, 0, &a.pubkey())], &[&w.payer]), 6012); // PoolNotSettled
    // winners: A, bot, B, D (index 3 is past payout_amounts → nothing to claim)
    assert_eq!(outcome(send(&mut w.svm, &[finish(1, vec![Some(a.pubkey()), None, Some(b.pubkey()), Some(d.pubkey())])], &[&w.payer])), 0);
    assert_err(&send(&mut w.svm, &[claim_sol(1, 0, &b.pubkey())], &[&w.payer]), 6013); // wrong winner for index
    assert_err(&send(&mut w.svm, &[claim_sol(1, 1, &b.pubkey())], &[&w.payer]), 6013); // index is a bot
    assert_err(&send(&mut w.svm, &[claim_sol(1, 3, &d.pubkey())], &[&w.payer]), 6014); // NothingToClaim
    ok(send(&mut w.svm, &[claim_sol(1, 0, &a.pubkey())], &[&w.payer]));
    assert_err(&send(&mut w.svm, &[claim_sol(1, 0, &a.pubkey())], &[&w.payer]), 6015); // AlreadyClaimed
}

#[test]
fn fee_return_refund_close_guards() {
    let mut w = world();
    let a = funded(&mut w.svm);
    let booster = funded(&mut w.svm);
    let tok = fungible(&mut w.svm, &w.payer, &booster.pubkey(), &TOKEN, 1_000);
    create_ata(&mut w.svm, &w.payer, &a.pubkey(), &tok, &TOKEN);
    // NFT that DOES have a Treasury ATA (like a Token Metadata NFT) → handler returns NoFeeForNft.
    let n = nft(&mut w.svm, &w.payer, &booster.pubkey());
    create_ata(&mut w.svm, &w.payer, &treasury_pda().0, &n, &TOKEN);

    ok(send(&mut w.svm, &[start_match_sol(1, vec![2_000_000, 1_000_000])], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_sol(1, &booster.pubkey(), SOL / 10, ""), boost_token(1, &booster.pubkey(), &tok, &TOKEN, 100, ""), boost_token(1, &booster.pubkey(), &n, &TOKEN, 1, "")], &[&booster]));
    assert_err(&send(&mut w.svm, &[close_pool(1)], &[&w.payer]), 6021); // InvalidStatus (Open)
    assert_eq!(outcome(send(&mut w.svm, &[finish(1, vec![Some(a.pubkey()), None])], &[&w.payer])), 0);

    ok(send(&mut w.svm, &[collect_fee_sol(1)], &[&w.payer]));
    assert_err(&send(&mut w.svm, &[collect_fee_sol(1)], &[&w.payer]), 6017); // FeeAlreadyCollected
    assert_err(&send(&mut w.svm, &[collect_fee_token(1, &n, &TOKEN)], &[&w.payer]), 6016); // NoFeeForNft
    ok(send(&mut w.svm, &[return_base_sol(1)], &[&w.payer]));
    assert_err(&send(&mut w.svm, &[return_base_sol(1)], &[&w.payer]), 6018); // BaseAlreadyReturned
    assert_err(&send(&mut w.svm, &[refund_sol(1, &booster.pubkey())], &[&w.payer]), 6019); // RefundNotAllowed (rank 1 exists)
    assert_err(&send(&mut w.svm, &[close_sol_asset(1, &game_authority())], &[&w.payer]), 6022); // AssetNotSettled (A unclaimed)
    assert_err(&send(&mut w.svm, &[close_pool(1)], &[&w.payer]), 6023); // PoolNotEmpty
    // Wrong variant for the asset kind (token asset passed to the SOL instruction).
    let p = pool_pda(1);
    let bad = rix(
        rewards_pool::accounts::ClaimSol { match_pool: p, pool_asset: asset_pda(&p, &tok), winner: a.pubkey() }.to_account_metas(None),
        rewards_pool::instruction::ClaimSol { index: 0 }.data(),
    );
    assert_err(&send(&mut w.svm, &[bad], &[&w.payer]), 6030); // AssetKindMismatch
}

/// dev-qa round-2 major: return_base_sol must not drain a SOL *boost* when the base is a token.
#[test]
fn return_base_cannot_drain_boost_sol_of_a_token_base_pool() {
    let mut w = world();
    let (a, booster) = (funded(&mut w.svm), funded(&mut w.svm));
    let point = fungible(&mut w.svm, &w.payer, &game_authority(), &TOKEN, 1_000);
    ok(send(&mut w.svm, &[start_match_token(1, &point, &TOKEN, vec![100, 50])], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_sol(1, &booster.pubkey(), SOL / 10, "")], &[&booster]));
    assert_eq!(outcome(send(&mut w.svm, &[finish(1, vec![Some(a.pubkey()), None])], &[&w.payer])), 0);
    let p = pool_pda(1);
    let drain = rix(
        rewards_pool::accounts::ReturnBaseSol {
            match_pool: p,
            pool_asset: asset_pda(&p, &Pubkey::default()),
            base_source: ata(&game_authority(), &point, &TOKEN), // = match_pool.base_source
        }
        .to_account_metas(None),
        rewards_pool::instruction::ReturnBaseSol {}.data(),
    );
    assert_err(&send(&mut w.svm, &[drain], &[&w.payer]), 6032); // NotBaseAsset
}

// ─────────── real Token Metadata NFT (fixture from devnet — scripts/fetch-fixtures.sh) ───────────

fn borsh_str(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u32).to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

#[test]
fn token_metadata_nft_is_boosted_as_nft_and_paid_to_rank_one() {
    let mut w = world();
    let tm = rewards_pool::constants::TOKEN_METADATA_ID;
    let path = format!("{}/../../tests/fixtures/mpl_token_metadata.so", env!("CARGO_MANIFEST_DIR"));
    w.svm.add_program(tm, &std::fs::read(&path).expect("run scripts/fetch-fixtures.sh")).unwrap();
    let (booster, a) = (funded(&mut w.svm), funded(&mut w.svm));

    let mint = create_mint(&mut w.svm, &booster, &TOKEN, 0, &[]);
    let b_ata = create_ata(&mut w.svm, &booster, &booster.pubkey(), &mint, &TOKEN);
    mint_to(&mut w.svm, &booster, &mint, &b_ata, &TOKEN, 1);
    let metadata = pda(&[b"metadata", tm.as_ref(), mint.as_ref()], &tm);
    let edition = pda(&[b"metadata", tm.as_ref(), mint.as_ref(), b"edition"], &tm);
    let bk = booster.pubkey();
    let mut d = vec![33u8];
    borsh_str(&mut d, "Hub NFT");
    borsh_str(&mut d, "HUB");
    borsh_str(&mut d, "https://example.com/hub.json");
    d.extend_from_slice(&0u16.to_le_bytes());
    d.extend_from_slice(&[0, 0, 0, 1, 0]);
    let md = Instruction { program_id: tm, accounts: vec![AccountMeta::new(metadata, false), AccountMeta::new_readonly(mint, false), AccountMeta::new_readonly(bk, true), AccountMeta::new(bk, true), AccountMeta::new_readonly(bk, true), AccountMeta::new_readonly(system_program::ID, false)], data: d };
    let mut e = vec![17u8, 1];
    e.extend_from_slice(&0u64.to_le_bytes());
    let me = Instruction { program_id: tm, accounts: vec![AccountMeta::new(edition, false), AccountMeta::new(mint, false), AccountMeta::new_readonly(bk, true), AccountMeta::new_readonly(bk, true), AccountMeta::new(bk, true), AccountMeta::new(metadata, false), AccountMeta::new_readonly(TOKEN, false), AccountMeta::new_readonly(system_program::ID, false)], data: e };
    ok(send(&mut w.svm, &[md, me], &[&booster]));
    let mint_acc = w.svm.get_account(&mint).unwrap();
    let st = StateWithExtensions::<MintState>::unpack(&mint_acc.data).unwrap();
    assert_eq!(Option::<Pubkey>::from(st.base.mint_authority), Some(edition));
    // platform-core gives it a Treasury ATA (not pure 1/1 there) — harmless.
    allow_mint(&mut w.svm, &w.payer, &mint, &TOKEN, 0, true);

    ok(send(&mut w.svm, &[start_match(1)], &[&w.payer]));
    ok(send(&mut w.svm, &[boost_token(1, &bk, &mint, &TOKEN, 1, "rare!")], &[&booster]));
    assert_eq!(asset(&w.svm, &asset_pda(&pool_pda(1), &mint)).kind, AssetKind::Nft, "TM NFT classified as Nft (spec §3.4)");
    assert_eq!(outcome(send(&mut w.svm, &[finish(1, vec![Some(a.pubkey())])], &[&w.payer])), 0);
    ok(send(&mut w.svm, &[claim_token_with_ata(1, 0, &a.pubkey(), &a.pubkey(), &mint, &TOKEN)], &[&a]));
    assert_eq!(token_balance(&w.svm, &ata(&a.pubkey(), &mint, &TOKEN)), 1);
    ok(send(&mut w.svm, &[close_receipt(1, &bk, &mint), close_token_asset(1, &mint, &TOKEN, &bk), close_pool(1)], &[&w.payer]));
}
