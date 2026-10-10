//! Shared LiteSVM harness. Platform-core accounts are written directly (owner =
//! platform-core ID, layout from idls/platform_core.json) — rewards-pool only *reads*
//! them, so platform-core's program is not loaded.
#![allow(dead_code, clippy::result_large_err, clippy::too_many_arguments)]

use {
    anchor_lang::{
        prelude::{Clock, Pubkey},
        solana_program::{instruction::Instruction, system_instruction, system_program},
        AccountDeserialize, InstructionData, ToAccountMetas,
    },
    anchor_spl::{
        associated_token::{get_associated_token_address_with_program_id as ata_of, spl_associated_token_account::instruction::create_associated_token_account},
        token_2022::spl_token_2022::{
            self,
            extension::{ExtensionType, StateWithExtensions},
            instruction as t22_ix,
            state::{Account as TokenAccountState, Mint as MintState},
        },
    },
    litesvm::{types::TransactionResult, LiteSVM},
    rewards_pool::{constants::*, state::*},
    solana_account::Account,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

pub const SOL: u64 = 1_000_000_000;
pub const TOKEN: Pubkey = anchor_spl::token::ID;
pub const TOKEN_2022: Pubkey = anchor_spl::token_2022::ID;
pub const ATA_PROGRAM: Pubkey = anchor_spl::associated_token::ID;
pub const PC: Pubkey = rewards_pool::platform_core::ID;
pub const GAME: Pubkey = mock_game::ID;
pub const FEE_BPS: u16 = 100; // 1% (DECISIONS rev 7)
pub const TOKEN_ACCOUNT_LEN: usize = 165;

pub fn program_so(name: &str) -> Vec<u8> {
    let path = format!("{}/../deploy/{name}.so", env!("CARGO_TARGET_TMPDIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e} — build first: ./scripts/test.sh"))
}

pub fn pda(seeds: &[&[u8]], program: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(seeds, program).0
}
pub fn game_authority() -> Pubkey {
    pda(&[GAME_AUTHORITY_SEED], &GAME)
}
pub fn pool_pda(room_id: u64) -> Pubkey {
    pda(&[MATCH_POOL_SEED, GAME.as_ref(), &room_id.to_le_bytes()], &rewards_pool::ID)
}
pub fn asset_pda(pool: &Pubkey, asset_id: &Pubkey) -> Pubkey {
    pda(&[POOL_ASSET_SEED, pool.as_ref(), asset_id.as_ref()], &rewards_pool::ID)
}
pub fn vault_pda(asset: &Pubkey) -> Pubkey {
    pda(&[VAULT_SEED, asset.as_ref()], &rewards_pool::ID)
}
pub fn receipt_pda(pool: &Pubkey, booster: &Pubkey, asset_id: &Pubkey) -> Pubkey {
    pda(&[BOOST_RECEIPT_SEED, pool.as_ref(), booster.as_ref(), asset_id.as_ref()], &rewards_pool::ID)
}
pub fn config_pda() -> Pubkey {
    pda(&[PC_CONFIG_SEED], &PC)
}
pub fn treasury_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[PC_TREASURY_SEED], &PC)
}
pub fn game_entry_pda() -> Pubkey {
    pda(&[PC_GAME_SEED, GAME.as_ref()], &PC)
}
pub fn allowed_mint_pda(mint: &Pubkey) -> Pubkey {
    pda(&[PC_BOOST_MINT_SEED, mint.as_ref()], &PC)
}
pub fn ata(owner: &Pubkey, mint: &Pubkey, tp: &Pubkey) -> Pubkey {
    ata_of(owner, mint, tp)
}

// ─────────────────── raw platform-core accounts ───────────────────

fn disc(name: &str) -> [u8; 8] {
    match name {
        "AllowedMint" => [173, 229, 179, 46, 121, 164, 247, 6],
        "GameEntry" => [119, 212, 18, 192, 14, 188, 27, 149],
        "PlatformConfig" => [160, 78, 128, 0, 248, 83, 230, 160],
        "Treasury" => [238, 239, 123, 238, 89, 1, 168, 253],
        _ => unreachable!(),
    }
}

pub fn put(svm: &mut LiteSVM, addr: Pubkey, owner: Pubkey, data: Vec<u8>) {
    let lamports = svm.minimum_balance_for_rent_exemption(data.len());
    svm.set_account(addr, Account { lamports, data, owner, executable: false, rent_epoch: 0 }).unwrap();
}

pub fn put_game_entry(svm: &mut LiteSVM, suspended: bool) {
    let (ga, ga_bump) = Pubkey::find_program_address(&[GAME_AUTHORITY_SEED], &GAME);
    let bump = Pubkey::find_program_address(&[PC_GAME_SEED, GAME.as_ref()], &PC).1;
    let mut d = disc("GameEntry").to_vec();
    d.extend([1, bump, suspended as u8]);
    d.extend(GAME.as_ref());
    d.extend(Pubkey::new_unique().as_ref()); // owner
    d.extend(ga.as_ref());
    d.push(ga_bump);
    d.extend(Pubkey::new_unique().as_ref()); // registered_by
    d.extend(0i64.to_le_bytes());
    d.extend(0i64.to_le_bytes());
    d.extend([0u8; 64]);
    put(svm, game_entry_pda(), PC, d);
}

pub fn put_config(svm: &mut LiteSVM, boost_fee_bps: u16) {
    let bump = Pubkey::find_program_address(&[PC_CONFIG_SEED], &PC).1;
    let (_, tb) = treasury_pda();
    let mut d = disc("PlatformConfig").to_vec();
    d.extend([1, bump]);
    d.extend(boost_fee_bps.to_le_bytes());
    d.extend(100u16.to_le_bytes()); // ad_fee_bps
    d.extend(1u16.to_le_bytes()); // admin_count
    d.push(tb);
    d.extend(0i64.to_le_bytes());
    d.extend(0i64.to_le_bytes());
    d.extend([0u8; 64]);
    put(svm, config_pda(), PC, d);
    let mut t = disc("Treasury").to_vec();
    t.extend([1, tb]);
    t.extend([0u8; 32]);
    put(svm, treasury_pda().0, PC, t);
}

/// platform-core `allow_mint`: AllowedMint PDA + Treasury ATA (X3, skipped for pure 1/1).
pub fn allow_mint(svm: &mut LiteSVM, payer: &Keypair, mint: &Pubkey, tp: &Pubkey, decimals: u8, treasury_ata: bool) {
    let bump = Pubkey::find_program_address(&[PC_BOOST_MINT_SEED, mint.as_ref()], &PC).1;
    let mut d = disc("AllowedMint").to_vec();
    d.extend([1, bump]);
    d.extend(mint.as_ref());
    d.extend(tp.as_ref());
    d.push(decimals);
    d.extend(payer.pubkey().as_ref());
    d.extend(0i64.to_le_bytes());
    d.extend([0u8; 32]);
    put(svm, allowed_mint_pda(mint), PC, d);
    if treasury_ata {
        create_ata(svm, payer, &treasury_pda().0, mint, tp);
    }
}

/// platform-core `disallow_mint` (closes the AllowedMint; Treasury ATA stays).
pub fn disallow_mint(svm: &mut LiteSVM, mint: &Pubkey) {
    svm.set_account(allowed_mint_pda(mint), Account::default()).unwrap();
}

// ─────────────────── world ───────────────────

pub struct World {
    pub svm: LiteSVM,
    pub payer: Keypair,
}

pub fn world() -> World {
    let mut svm = LiteSVM::new();
    svm.add_program(rewards_pool::ID, &program_so("rewards_pool")).unwrap();
    svm.add_program(GAME, &program_so("mock_game")).unwrap();
    put_game_entry(&mut svm, false);
    put_config(&mut svm, FEE_BPS);
    svm.airdrop(&game_authority(), 10 * SOL).unwrap();
    let payer = Keypair::new();
    svm.airdrop(&payer.pubkey(), 1_000 * SOL).unwrap();
    World { svm, payer }
}

pub fn funded(svm: &mut LiteSVM) -> Keypair {
    let kp = Keypair::new();
    svm.airdrop(&kp.pubkey(), 100 * SOL).unwrap();
    kp
}

pub fn send(svm: &mut LiteSVM, ixs: &[Instruction], signers: &[&Keypair]) -> TransactionResult {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&signers[0].pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), signers).unwrap();
    let res = svm.send_transaction(tx);
    svm.expire_blockhash();
    res
}

pub fn ok(res: TransactionResult) -> litesvm::types::TransactionMetadata {
    res.unwrap_or_else(|e| panic!("tx failed: {:?}\n{:#?}", e.err, e.meta.logs))
}

pub fn assert_err(res: &TransactionResult, code: u32) {
    let e = res.as_ref().expect_err("expected failure");
    let s = format!("{:?}", e.err);
    assert!(s.contains(&format!("Custom({code})")), "expected Custom({code}), got {s}\n{:#?}", e.meta.logs);
}

pub fn lamports(svm: &LiteSVM, a: &Pubkey) -> u64 {
    svm.get_account(a).map_or(0, |x| x.lamports)
}
pub fn gone(svm: &LiteSVM, a: &Pubkey) -> bool {
    svm.get_account(a).is_none_or(|x| x.data.is_empty() && x.lamports == 0)
}
pub fn token_balance(svm: &LiteSVM, a: &Pubkey) -> u64 {
    svm.get_account(a).map_or(0, |x| StateWithExtensions::<TokenAccountState>::unpack(&x.data).unwrap().base.amount)
}
pub fn pool(svm: &LiteSVM, room_id: u64) -> MatchPool {
    MatchPool::try_deserialize(&mut &svm.get_account(&pool_pda(room_id)).unwrap().data[..]).unwrap()
}
pub fn asset(svm: &LiteSVM, a: &Pubkey) -> PoolAsset {
    PoolAsset::try_deserialize(&mut &svm.get_account(a).unwrap().data[..]).unwrap()
}
pub fn set_time(svm: &mut LiteSVM, unix: i64) {
    let mut c: Clock = svm.get_sysvar();
    c.unix_timestamp = unix;
    svm.set_sysvar(&c);
}

// ─────────────────── tokens ───────────────────

pub fn create_mint(svm: &mut LiteSVM, payer: &Keypair, tp: &Pubkey, decimals: u8, exts: &[ExtensionType]) -> Pubkey {
    let mint = Keypair::new();
    let space = ExtensionType::try_calculate_account_len::<MintState>(exts).unwrap();
    let rent = svm.minimum_balance_for_rent_exemption(space);
    let mut ixs = vec![system_instruction::create_account(&payer.pubkey(), &mint.pubkey(), rent, space as u64, tp)];
    for e in exts {
        if *e == ExtensionType::Pausable {
            ixs.push(spl_token_2022::extension::pausable::instruction::initialize(tp, &mint.pubkey(), &payer.pubkey()).unwrap());
        }
    }
    ixs.push(t22_ix::initialize_mint2(tp, &mint.pubkey(), &payer.pubkey(), Some(&payer.pubkey()), decimals).unwrap());
    ok(send(svm, &ixs, &[payer, &mint]));
    mint.pubkey()
}

pub fn create_ata(svm: &mut LiteSVM, payer: &Keypair, owner: &Pubkey, mint: &Pubkey, tp: &Pubkey) -> Pubkey {
    ok(send(svm, &[create_associated_token_account(&payer.pubkey(), owner, mint, tp)], &[payer]));
    ata(owner, mint, tp)
}

pub fn mint_to(svm: &mut LiteSVM, payer: &Keypair, mint: &Pubkey, to: &Pubkey, tp: &Pubkey, amount: u64) {
    ok(send(svm, &[t22_ix::mint_to(tp, mint, to, &payer.pubkey(), &[], amount).unwrap()], &[payer]));
}

/// Fungible mint (decimals 6) allowlisted, with `amount` minted to `holder`'s ATA.
pub fn fungible(svm: &mut LiteSVM, payer: &Keypair, holder: &Pubkey, tp: &Pubkey, amount: u64) -> Pubkey {
    let mint = create_mint(svm, payer, tp, 6, &[]);
    allow_mint(svm, payer, &mint, tp, 6, true);
    let a = create_ata(svm, payer, holder, &mint, tp);
    mint_to(svm, payer, &mint, &a, tp, amount);
    mint
}

/// Pure 1/1 NFT (no mint authority) owned by `holder`, allowlisted without Treasury ATA.
pub fn nft(svm: &mut LiteSVM, payer: &Keypair, holder: &Pubkey) -> Pubkey {
    let mint = create_mint(svm, payer, &TOKEN, 0, &[]);
    let a = create_ata(svm, payer, holder, &mint, &TOKEN);
    mint_to(svm, payer, &mint, &a, &TOKEN, 1);
    ok(send(svm, &[t22_ix::set_authority(&TOKEN, &mint, None, t22_ix::AuthorityType::MintTokens, &payer.pubkey(), &[]).unwrap()], &[payer]));
    allow_mint(svm, payer, &mint, &TOKEN, 0, false);
    mint
}

// ─────────────────── game instructions (via mock-game CPI) ───────────────────

pub fn gix(accounts: Vec<anchor_lang::solana_program::instruction::AccountMeta>, data: Vec<u8>) -> Instruction {
    Instruction { program_id: GAME, accounts, data }
}

pub fn start_match(room_id: u64) -> Instruction {
    gix(
        mock_game::accounts::StartMatch {
            game_authority: game_authority(),
            this_program: GAME,
            game_entry: game_entry_pda(),
            config: config_pda(),
            match_pool: pool_pda(room_id),
            rewards_pool_program: rewards_pool::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        mock_game::instruction::StartMatch { room_id }.data(),
    )
}

pub fn start_match_sol(room_id: u64, payout_amounts: Vec<u64>) -> Instruction {
    let p = pool_pda(room_id);
    gix(
        mock_game::accounts::StartMatchSol {
            game_authority: game_authority(),
            this_program: GAME,
            game_entry: game_entry_pda(),
            config: config_pda(),
            match_pool: p,
            pool_asset: asset_pda(&p, &Pubkey::default()),
            rewards_pool_program: rewards_pool::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        mock_game::instruction::StartMatchSol { room_id, payout_amounts }.data(),
    )
}

pub fn start_match_token(room_id: u64, mint: &Pubkey, tp: &Pubkey, payout_amounts: Vec<u64>) -> Instruction {
    let p = pool_pda(room_id);
    let a = asset_pda(&p, mint);
    gix(
        mock_game::accounts::StartMatchToken {
            game_authority: game_authority(),
            this_program: GAME,
            game_entry: game_entry_pda(),
            config: config_pda(),
            match_pool: p,
            base_mint: *mint,
            allowed_mint: allowed_mint_pda(mint),
            base_source: ata(&game_authority(), mint, tp),
            pool_asset: a,
            vault: vault_pda(&a),
            token_program: *tp,
            associated_token_program: ATA_PROGRAM,
            rewards_pool_program: rewards_pool::ID,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        mock_game::instruction::StartMatchToken { room_id, payout_amounts }.data(),
    )
}

fn game_call_accounts(room_id: u64, pool: Pubkey) -> Vec<anchor_lang::solana_program::instruction::AccountMeta> {
    let _ = room_id;
    mock_game::accounts::GameCallAccounts {
        game_authority: game_authority(),
        this_program: GAME,
        match_pool: pool,
        rewards_pool_program: rewards_pool::ID,
    }
    .to_account_metas(None)
}
pub fn lock(room_id: u64) -> Instruction {
    gix(game_call_accounts(room_id, pool_pda(room_id)), mock_game::instruction::Lock { room_id }.data())
}
pub fn finish(room_id: u64, winners: Vec<Option<Pubkey>>) -> Instruction {
    finish_with_pool(room_id, pool_pda(room_id), winners)
}
pub fn finish_with_pool(room_id: u64, pool: Pubkey, winners: Vec<Option<Pubkey>>) -> Instruction {
    gix(game_call_accounts(room_id, pool), mock_game::instruction::FinishMatch { room_id, winners }.data())
}
pub fn cancel_match(room_id: u64) -> Instruction {
    gix(game_call_accounts(room_id, pool_pda(room_id)), mock_game::instruction::CancelMatch { room_id }.data())
}

/// The u8 the mock game returned = what the game saw from `CpiOutcome`.
pub fn outcome(res: TransactionResult) -> u8 {
    let meta = ok(res);
    assert_eq!(meta.return_data.program_id, GAME);
    meta.return_data.data[0]
}

// ─────────────────── rewards-pool instructions ───────────────────

pub fn rix(accounts: Vec<anchor_lang::solana_program::instruction::AccountMeta>, data: Vec<u8>) -> Instruction {
    Instruction { program_id: rewards_pool::ID, accounts, data }
}

pub fn boost_sol(room_id: u64, booster: &Pubkey, amount: u64, message: &str) -> Instruction {
    let p = pool_pda(room_id);
    rix(
        rewards_pool::accounts::BoostSol {
            booster: *booster,
            match_pool: p,
            pool_asset: asset_pda(&p, &Pubkey::default()),
            boost_receipt: receipt_pda(&p, booster, &Pubkey::default()),
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        rewards_pool::instruction::BoostSol { amount, message: message.to_string() }.data(),
    )
}

pub fn boost_token(room_id: u64, booster: &Pubkey, mint: &Pubkey, tp: &Pubkey, amount: u64, message: &str) -> Instruction {
    let p = pool_pda(room_id);
    let a = asset_pda(&p, mint);
    rix(
        rewards_pool::accounts::BoostToken {
            booster: *booster,
            match_pool: p,
            mint: *mint,
            allowed_mint: allowed_mint_pda(mint),
            booster_token: ata(booster, mint, tp),
            pool_asset: a,
            vault: vault_pda(&a),
            boost_receipt: receipt_pda(&p, booster, mint),
            token_program: *tp,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        rewards_pool::instruction::BoostToken { amount, message: message.to_string() }.data(),
    )
}

pub fn expire(room_id: u64) -> Instruction {
    rix(rewards_pool::accounts::ExpirePool { match_pool: pool_pda(room_id) }.to_account_metas(None), rewards_pool::instruction::ExpirePool {}.data())
}

pub fn collect_fee_sol(room_id: u64) -> Instruction {
    let p = pool_pda(room_id);
    rix(
        rewards_pool::accounts::CollectFeeSol {
            match_pool: p,
            pool_asset: asset_pda(&p, &Pubkey::default()),
            config: config_pda(),
            treasury: treasury_pda().0,
        }
        .to_account_metas(None),
        rewards_pool::instruction::CollectFeeSol {}.data(),
    )
}

pub fn collect_fee_token(room_id: u64, mint: &Pubkey, tp: &Pubkey) -> Instruction {
    let p = pool_pda(room_id);
    let a = asset_pda(&p, mint);
    rix(
        rewards_pool::accounts::CollectFeeToken {
            match_pool: p,
            pool_asset: a,
            mint: *mint,
            vault: vault_pda(&a),
            config: config_pda(),
            treasury: treasury_pda().0,
            treasury_ata: ata(&treasury_pda().0, mint, tp),
            token_program: *tp,
        }
        .to_account_metas(None),
        rewards_pool::instruction::CollectFeeToken {}.data(),
    )
}

pub fn claim_sol(room_id: u64, index: u8, winner: &Pubkey) -> Instruction {
    let p = pool_pda(room_id);
    rix(
        rewards_pool::accounts::ClaimSol { match_pool: p, pool_asset: asset_pda(&p, &Pubkey::default()), winner: *winner }.to_account_metas(None),
        rewards_pool::instruction::ClaimSol { index }.data(),
    )
}

pub fn claim_token(room_id: u64, index: u8, winner: &Pubkey, mint: &Pubkey, tp: &Pubkey) -> Instruction {
    let p = pool_pda(room_id);
    let a = asset_pda(&p, mint);
    rix(
        rewards_pool::accounts::ClaimToken {
            match_pool: p,
            pool_asset: a,
            mint: *mint,
            vault: vault_pda(&a),
            winner: *winner,
            winner_ata: ata(winner, mint, tp),
            token_program: *tp,
        }
        .to_account_metas(None),
        rewards_pool::instruction::ClaimToken { index }.data(),
    )
}

pub fn claim_token_with_ata(room_id: u64, index: u8, payer: &Pubkey, winner: &Pubkey, mint: &Pubkey, tp: &Pubkey) -> Instruction {
    let p = pool_pda(room_id);
    let a = asset_pda(&p, mint);
    rix(
        rewards_pool::accounts::ClaimTokenWithAta {
            payer: *payer,
            match_pool: p,
            pool_asset: a,
            mint: *mint,
            vault: vault_pda(&a),
            winner: *winner,
            winner_ata: ata(winner, mint, tp),
            token_program: *tp,
            associated_token_program: ATA_PROGRAM,
            system_program: system_program::ID,
        }
        .to_account_metas(None),
        rewards_pool::instruction::ClaimTokenWithAta { index }.data(),
    )
}

pub fn return_base_sol(room_id: u64) -> Instruction {
    let p = pool_pda(room_id);
    rix(
        rewards_pool::accounts::ReturnBaseSol { match_pool: p, pool_asset: asset_pda(&p, &Pubkey::default()), base_source: game_authority() }
            .to_account_metas(None),
        rewards_pool::instruction::ReturnBaseSol {}.data(),
    )
}

pub fn return_base_token(room_id: u64, mint: &Pubkey, tp: &Pubkey) -> Instruction {
    let p = pool_pda(room_id);
    let a = asset_pda(&p, mint);
    rix(
        rewards_pool::accounts::ReturnBaseToken {
            match_pool: p,
            pool_asset: a,
            mint: *mint,
            vault: vault_pda(&a),
            base_source: ata(&game_authority(), mint, tp),
            token_program: *tp,
        }
        .to_account_metas(None),
        rewards_pool::instruction::ReturnBaseToken {}.data(),
    )
}

pub fn refund_sol(room_id: u64, booster: &Pubkey) -> Instruction {
    let p = pool_pda(room_id);
    rix(
        rewards_pool::accounts::RefundBoostSol {
            match_pool: p,
            pool_asset: asset_pda(&p, &Pubkey::default()),
            boost_receipt: receipt_pda(&p, booster, &Pubkey::default()),
            booster: *booster,
        }
        .to_account_metas(None),
        rewards_pool::instruction::RefundBoostSol {}.data(),
    )
}

pub fn refund_token(room_id: u64, booster: &Pubkey, mint: &Pubkey, tp: &Pubkey) -> Instruction {
    let p = pool_pda(room_id);
    let a = asset_pda(&p, mint);
    rix(
        rewards_pool::accounts::RefundBoostToken {
            match_pool: p,
            pool_asset: a,
            mint: *mint,
            vault: vault_pda(&a),
            boost_receipt: receipt_pda(&p, booster, mint),
            booster: *booster,
            booster_ata: ata(booster, mint, tp),
            token_program: *tp,
        }
        .to_account_metas(None),
        rewards_pool::instruction::RefundBoostToken {}.data(),
    )
}

pub fn close_receipt(room_id: u64, booster: &Pubkey, asset_id: &Pubkey) -> Instruction {
    let p = pool_pda(room_id);
    rix(
        rewards_pool::accounts::CloseReceipt { match_pool: p, boost_receipt: receipt_pda(&p, booster, asset_id), rent_payer: *booster }
            .to_account_metas(None),
        rewards_pool::instruction::CloseReceipt {}.data(),
    )
}

pub fn close_sol_asset(room_id: u64, rent_payer: &Pubkey) -> Instruction {
    let p = pool_pda(room_id);
    rix(
        rewards_pool::accounts::CloseSolAsset { match_pool: p, pool_asset: asset_pda(&p, &Pubkey::default()), rent_payer: *rent_payer }
            .to_account_metas(None),
        rewards_pool::instruction::CloseSolAsset {}.data(),
    )
}

pub fn close_token_asset(room_id: u64, mint: &Pubkey, tp: &Pubkey, rent_payer: &Pubkey) -> Instruction {
    let p = pool_pda(room_id);
    let a = asset_pda(&p, mint);
    rix(
        rewards_pool::accounts::CloseTokenAsset { match_pool: p, pool_asset: a, mint: *mint, vault: vault_pda(&a), rent_payer: *rent_payer, token_program: *tp }
            .to_account_metas(None),
        rewards_pool::instruction::CloseTokenAsset {}.data(),
    )
}

pub fn close_pool(room_id: u64) -> Instruction {
    rix(
        rewards_pool::accounts::ClosePool { match_pool: pool_pda(room_id), rent_payer: game_authority() }.to_account_metas(None),
        rewards_pool::instruction::ClosePool {}.data(),
    )
}
