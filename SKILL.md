---
name: rewards-pool-integration
description: วิธีต่อเกม (Anchor program) เข้ากับ rewards-pool ของ blockchain-games-hub — เปิด pool รางวัลต่อ match ผ่าน CPI, ส่งผลอันดับ, ให้ผู้เล่น claim และให้ใครก็ Boost รางวัลได้ · ใช้เมื่อเขียน/รีวิว game program ที่ต้องแจกรางวัล หรือ FE ที่ทำปุ่ม Boost / Claim
---

# rewards-pool — Integration Skill (สำหรับเกม)

> program ที่ deploy บน **devnet** แล้ว — เกมของคุณ CPI เข้ามาได้เลย ไม่ต้อง deploy rewards-pool เอง
> ที่มา: [`specs/spec-v2.md`](specs/spec-v2.md) (สัญญาเต็ม) · [`idls/rewards_pool.json`](idls/rewards_pool.json) · ตัวอย่างเกมที่รันจริง: [`tests/mock-game/src/lib.rs`](tests/mock-game/src/lib.rs) · มติ: `../DECISIONS.md` (R1–R33)
> ⚠️ devnet เท่านั้น

---

## 0. TL;DR

1. ให้ Admin ของ platform-core **ลงทะเบียนเกม** (`GameEntry` Active) — platform-core จะบันทึก `game_authority` = PDA `[b"game_authority"]` ของ program คุณ
2. **เติม SOL ให้ `game_authority` PDA** — มันจ่าย rent ของ pool (~0.0087 SOL ต่อ match ที่เปิดพร้อมกันบน devnet) และได้คืนตอนปิด
3. copy `idls/rewards_pool.json` → `idls/` ของ repo คุณ แล้ว `declare_program!(rewards_pool);`
4. ต้น match: CPI `open_pool*` · ปิดรับ boost: `lock_boosts` (ไม่บังคับ) · จบ match: CPI `settle(room_id, winners)` แล้ว **อ่าน `CpiOutcome` ทุกครั้ง** · ยกเลิก: `cancel`
5. ที่เหลือ (boost, claim, คืนเงิน, ปิด account) เป็นของผู้เล่น / booster / crank — **เกมไม่ต้องทำอะไร**

---

## 1. Addresses (devnet)

| อะไร | ค่า |
|------|-----|
| **rewards-pool program** | `HgApfwjAg1L2FJL3fX7A4WX5yfhrVV8k9Ahii5Pn8HVW` |
| IDL | `idls/rewards_pool.json` · **v1.0.0** (23 ix · error 6000–6034 · 10 event) |
| deploy slot | `509454201` (upgradeable · authority `HRstpEJNgPzXUPMQmDFef4HzAfydbqK6KorqFuzMsTvy`) |
| platform-core program | `AJnPSYsn7J2muHfKztRtpZz5kBP94dWkNZLr9aMhu18G` |
| `PlatformConfig` `[b"config"]` | `Be8StYkiu7eBMb6m3TvG7QzMZpFL69bcn1kGmZwSmrJ7` (boost fee 100 bps = 1%) |
| `Treasury` `[b"treasury"]` | `CBcvwWyNbXKpk5GK6VczDBqaZrP1qsvBzetv7Ms5A6pz` |
| Token / Token-2022 / ATA | มาตรฐาน (`anchor_spl::token::ID`, `token_2022::ID`, `associated_token::ID`) |

ไม่มี `initialize` — ใช้ได้ทันทีเมื่อเกมลงทะเบียนแล้ว · IDL ไม่ได้เก็บบน chain → ใช้ไฟล์ใน repo

---

## 2. ก่อนเริ่ม (checklist)

- [ ] เกมมี `GameEntry` **Active** ใน platform-core (`[b"game", <your_program_id>]` owner = platform-core) — ขอ Admin (maintainer)
- [ ] `game_authority` = `Pubkey::find_program_address(&[b"game_authority"], &your_program_id)` — เป็นบัญชี System ธรรมดา (ไม่มี data) **ที่มี SOL**
  - หลังจ่าย rent ต้องเหลือ **0 หรือ ≥ rent-exempt** (devnet 0 byte = 650,240 lamports) ไม่งั้น tx fail
- [ ] ถ้าให้รางวัลพื้นฐานเป็น token: mint ต้องอยู่ใน **allowlist ของ platform-core** (`AllowedMint` `[b"boost_mint", mint]`) และ `game_authority` ต้องมี **ATA** ของ mint นั้นพร้อมยอดพอ
- [ ] `room_id` (u64) **ห้ามซ้ำ**ต่อ game program (ใช้ `room_id` ของ lobby — เพิ่มอย่างเดียว)
- [ ] จำนวนอันดับที่มีรางวัล ≤ **32**
- [ ] Anchor **1.2.1** · TS client `@anchor-lang/core` 1.2.1 (ห้าม `@coral-xyz/anchor` กับ Anchor 1.x)

---

## 3. Setup

```toml
# Cargo.toml ของ game program
[dependencies]
anchor-lang = "1.2.1"
```

```rust
// idls/rewards_pool.json ต้องอยู่ใน directory บรรพบุรุษที่ใกล้ที่สุดของ crate
declare_program!(rewards_pool);
use rewards_pool::types::{CancelReason, CpiOutcome, NoOpReason};

pub const GAME_AUTHORITY_SEED: &[u8] = b"game_authority";
```

⚠️ type ที่ `declare_program!` สร้าง **ไม่มี `PartialEq`** → ใช้ `match` / `matches!` ไม่ใช่ `==`

### PDAs (seeds ทั้งหมด program = rewards-pool เว้นที่ระบุ)

| account | seeds |
|---------|-------|
| `match_pool` | `[b"match_pool", game_program, room_id.to_le_bytes()]` |
| `pool_asset` | `[b"pool_asset", match_pool, asset_id]` — SOL: `asset_id = Pubkey::default()` (32 ศูนย์) · token/NFT: mint |
| `vault` (token) | `[b"vault", pool_asset]` (token account · authority = `pool_asset`) |
| `boost_receipt` | `[b"boost", match_pool, booster, asset_id]` |
| `game_entry` | `[b"game", game_program]` · program = **platform-core** |
| `config` / `treasury` | `[b"config"]` / `[b"treasury"]` · program = **platform-core** |
| `allowed_mint` | `[b"boost_mint", mint]` · program = **platform-core** |

```ts
// FE / client (ใช้ @solana/web3.js 1.x)
const RP = new PublicKey("HgApfwjAg1L2FJL3fX7A4WX5yfhrVV8k9Ahii5Pn8HVW");
const le8 = (n: bigint) => { const b = Buffer.alloc(8); b.writeBigUInt64LE(n); return b; };
const [matchPool] = PublicKey.findProgramAddressSync([Buffer.from("match_pool"), gameProgram.toBuffer(), le8(roomId)], RP);
const [poolAsset] = PublicKey.findProgramAddressSync([Buffer.from("pool_asset"), matchPool.toBuffer(), (mint ?? PublicKey.default).toBuffer()], RP);
const [vault]     = PublicKey.findProgramAddressSync([Buffer.from("vault"), poolAsset.toBuffer()], RP);
```

---

## 4. ฝั่งเกม — CPI (signer = `game_authority` PDA ของเกม)

### 4.1 วงจรของ pool

```
open_pool* ──► Open ──(boost จากใครก็ได้)──► lock_boosts ──► BoostsLocked
                 │                                              │
                 └──────────────── settle ──────────────────────┴──► Settled ──► claim / ปิด account
                 └── cancel / expire_pool (7 วัน) / settle(ทุกช่อง None) ──► Cancelled ──► คืนเงินทั้งหมด
```

### 4.2 เปิด pool — เลือก 1 ใน 3

| ix | args | accounts (w = writable · s = signer) |
|----|------|--------------------------------------|
| `open_pool` (ไม่มีรางวัลพื้นฐาน — มีแต่ boost) | `room_id: u64` | `game_authority`(ws) · `game_program` · `game_entry` · `config` · `match_pool`(w) · `system_program` |
| `open_pool_sol` | `room_id`, `payout_amounts: Vec<u64>` (lamports ต่ออันดับ · **แต่ละค่า ≥ rent-exempt**) | + `pool_asset`(w) (asset_id = default) |
| `open_pool_token` | `room_id`, `payout_amounts: Vec<u64>` (หน่วยเล็กสุดของ token ต่ออันดับ) | + `base_mint` · `allowed_mint` · `base_source`(w) = **ATA ของ game_authority** · `pool_asset`(w) · `vault`(w) · `token_program` · `associated_token_program` |

- `payout_amounts[i]` = รางวัลของอันดับ `i+1` (R7) · ยอดรวมถูกโอนออกจาก `game_authority` / ATA ของมันตอนเปิด · base **ไม่เสีย fee**
- `game_program` = program id ของเกมคุณ · `game_entry` / `config` = account ของ platform-core (§1)
- **`open_pool*` error ได้** (เกมไม่ Active, mint ไม่อยู่ใน allowlist, ยอดไม่พอ, room_id ซ้ำ) → simulate ก่อน

```rust
pub fn start_match_token(ctx: Context<StartMatchToken>, room_id: u64, payout_amounts: Vec<u64>) -> Result<()> {
    let seeds: &[&[u8]] = &[GAME_AUTHORITY_SEED, &[ctx.bumps.game_authority]];
    let a = &ctx.accounts;
    rewards_pool::cpi::open_pool_token(
        CpiContext::new_with_signer(
            rewards_pool::ID, // Anchor 1.2.1: arg แรก = program_id: Pubkey
            rewards_pool::cpi::accounts::OpenPoolToken {
                game_authority: a.game_authority.to_account_info(),
                game_program: a.this_program.to_account_info(),
                game_entry: a.game_entry.to_account_info(),
                config: a.config.to_account_info(),
                match_pool: a.match_pool.to_account_info(),
                base_mint: a.base_mint.to_account_info(),
                allowed_mint: a.allowed_mint.to_account_info(),
                base_source: a.base_source.to_account_info(),
                pool_asset: a.pool_asset.to_account_info(),
                vault: a.vault.to_account_info(),
                token_program: a.token_program.to_account_info(),
                associated_token_program: a.associated_token_program.to_account_info(),
                system_program: a.system_program.to_account_info(),
            },
            &[seeds],
        ),
        room_id,
        payout_amounts,
    )
}

#[derive(Accounts)]
pub struct StartMatchToken<'info> {
    #[account(mut, seeds = [GAME_AUTHORITY_SEED], bump)]
    pub game_authority: SystemAccount<'info>,
    /// CHECK: this program's id
    #[account(address = crate::ID)]
    pub this_program: UncheckedAccount<'info>,
    // game_entry, config, match_pool, base_mint, allowed_mint, base_source, pool_asset, vault,
    // token_program, associated_token_program: UncheckedAccount — rewards-pool ตรวจเองทั้งหมด
    // ...
    /// CHECK: rewards-pool program — ต้องอยู่ใน tx (CpiContext รับแค่ Pubkey)
    #[account(address = rewards_pool::ID)]
    pub rewards_pool_program: UncheckedAccount<'info>,
    pub system_program: Program<'info, System>,
}
```
ตัวอย่างเต็มทั้ง 3 แบบ: [`tests/mock-game/src/lib.rs`](tests/mock-game/src/lib.rs) (`start_match`, `start_match_sol`, `start_match_token`)

### 4.3 จบ match — `settle` (และ `lock_boosts` / `cancel`)

| ix | args | accounts | return |
|----|------|----------|--------|
| `settle` | `room_id: u64`, `winners: Vec<Option<Pubkey>>` | `game_authority`(s) · `game_program` · `match_pool`(w) | `CpiOutcome` |
| `lock_boosts` | `room_id` | เหมือนบน | `CpiOutcome` |
| `cancel` | `room_id` | เหมือนบน | `CpiOutcome` |

**`winners`**:
- `winners[0]` = อันดับ 1 → ได้ base อันดับ 1 **+ boost ทั้งหมด** (หัก fee 1% · NFT ไม่หัก) (R9)
- `winners[i]` = อันดับ `i+1` → ได้ `payout_amounts[i]`
- **`None`** = ช่องนี้ไม่มีผู้มีสิทธิ์ — **บอท / ผู้เล่นที่หลุด / AI Agent** (R16) → base ช่องนั้นคืนเกม · ถ้า `winners[0]` เป็น None → boost **คืน booster ทั้งหมด**
- **เกมตัดสิน tie เอง** — ลำดับใน Vec = ผล (R15) · pubkey ซ้ำได้ (W14)
- ทุกช่องเป็น `None` (หรือ Vec ว่าง) → pool ถูก **Cancelled** อัตโนมัติ (R13)
- ยาวเกิน 32 → ส่วนเกินถูกตัด · `Some(Pubkey::default())` / `Some(match_pool)` → ถือเป็น None

```rust
pub fn finish_match(ctx: Context<GameCall>, room_id: u64, winners: Vec<Option<Pubkey>>) -> Result<()> {
    let seeds: &[&[u8]] = &[GAME_AUTHORITY_SEED, &[ctx.bumps.game_authority]];
    let a = &ctx.accounts;
    let ret = rewards_pool::cpi::settle(
        CpiContext::new_with_signer(
            rewards_pool::ID,
            rewards_pool::cpi::accounts::Settle {
                game_authority: a.game_authority.to_account_info(),
                game_program: a.this_program.to_account_info(),
                match_pool: a.match_pool.to_account_info(),
            },
            &[seeds],
        ),
        room_id,
        winners,
    )?;
    match ret.get() {
        CpiOutcome::Done => { /* settle แล้ว — ผู้เล่น claim ได้ */ }
        CpiOutcome::NoOp(NoOpReason::Closed) => { /* pool ไม่มีอยู่ (ไม่เคยเปิด / ปิดไปแล้ว) */ }
        CpiOutcome::NoOp(_) => { /* settle / cancel ไปแล้ว — ไม่มีอะไรเปลี่ยน */ }
        CpiOutcome::Cancelled(CancelReason::NoEligibleWinner) => { /* ทุกช่อง None → cancel */ }
        CpiOutcome::Cancelled(_) => {}
    }
    Ok(())
}

#[derive(Accounts)]
pub struct GameCall<'info> {
    #[account(seeds = [GAME_AUTHORITY_SEED], bump)]
    pub game_authority: SystemAccount<'info>,
    /// CHECK: this program's id
    #[account(address = crate::ID)]
    pub this_program: UncheckedAccount<'info>,
    /// CHECK: rewards-pool ตรวจ address / owner เอง · อาจว่าง (pool ถูกปิด)
    #[account(mut)]
    pub match_pool: UncheckedAccount<'info>,
    /// CHECK: rewards-pool program
    #[account(address = rewards_pool::ID)]
    pub rewards_pool_program: UncheckedAccount<'info>,
}
```

### 4.4 กฎที่เกมต้องทำ (MUST)

1. **อ่าน `CpiOutcome` ทุกครั้ง** — ix ของเกม**ไม่ error เพราะสถานะของ pool** (กัน tx จบเกมล้ม) · `Ok` ≠ แจกรางวัลแล้ว (pool อาจถูก `expire_pool` ไปแล้วหลัง 7 วัน)
2. `cancel` ได้**ก่อน settle เท่านั้น** — หลัง settle ได้ `NoOp(AlreadySettled)`
3. "ไม่มีผู้ชนะ / เสมอทั้งหมด" = กติกาของเกม → `cancel` หรือ `settle` ที่ทุกช่อง None
4. ส่ง `match_pool` ของ `room_id` ให้ถูก — PDA ผิด → error `WrongMatchPool` (6033)
5. อย่าใส่ PDA ของ pool, program id หรือ sysvar เป็นผู้ชนะ — claim จะทำไม่ได้ตลอดไป (เกม = trust boundary ของปลายทาง)
6. ถ้าเกมรับ `winners` จาก client ใน tx ของตัวเอง ระวังขนาด: settle data สูงสุด ≈ 1,076 bytes (เพดาน tx 1,232)

---

## 5. ฝั่ง FE / ผู้เล่น / booster (เกมไม่ต้องเรียก)

### Boost — booster ใช้ **wallet ของตัวเอง** จ่าย gas เอง (ไม่ผ่าน relayer — W21)
| ix | args | accounts |
|----|------|----------|
| `boost_sol` | `amount` (≥ 0.001 SOL), `message` (≤ 140 **bytes**) | `booster`(ws) · `match_pool`(w) · `pool_asset`(w) · `boost_receipt`(w) · `system_program` |
| `boost_token` | `amount` (NFT = 1), `message` | `booster`(ws) · `match_pool`(w) · `mint` · `allowed_mint` · `booster_token`(w) · `pool_asset`(w) · `vault`(w) · `boost_receipt`(w) · `token_program` · `system_program` |

- ได้เฉพาะตอน **Open** · ทุก boost ไปที่**อันดับ 1** · mint ต้องอยู่ใน allowlist · pool รับได้ ≤ 8 ชนิด asset
- token ที่โอนเข้า vault ตรงๆ (ไม่ผ่าน `boost_token`) จะ**ถูก burn** ตอนปิด → ให้ boost ผ่านปุ่มเท่านั้น
- ข้อความอยู่ใน event `BoostAdded` (program log)

### Claim — **ไม่ต้องมี signer** · ปลายทางล็อกไว้ที่ผู้ชนะเสมอ · ไม่มี deadline (R12, R18)
| ix | args | accounts |
|----|------|----------|
| `claim_sol` | `index: u8` | `match_pool` · `pool_asset`(w) · `winner`(w) |
| `claim_token` | `index` | `match_pool` · `pool_asset`(w) · `mint` · `vault`(w) · `winner` · `winner_ata`(w) · `token_program` |
| `claim_token_with_ata` | `index` | `payer`(ws) + เหมือน `claim_token` + `associated_token_program` · `system_program` — สร้าง ATA ให้ (payer จ่าย rent) |

- `index` = ตำแหน่งใน `MatchPool.winners` (ไม่ใช่ pubkey) · 1 claim = 1 (index, asset)
- **Claim All**: ใส่หลาย claim ใน tx เดียว — วัดได้ **7 claim/tx** (mint เดียวกัน) · **5** (mint ต่างกัน) · tx atomic → simulate แล้วตัดตัวที่จะ fail ออกก่อน · ได้ `AlreadyClaimed` (6015) → อ่าน state ใหม่แล้ว build ใหม่
- **ผ่าน relayer** (ผู้เล่น social login): client ทำเครื่องหมาย account `winner` เป็น signer (3 ix ข้างบนคือที่ relayer sponsor)
- ไม่มี ATA → `claim_token` ได้ Anchor 3012 → ใช้ `claim_token_with_ata` (ผู้เล่นต้องมี SOL ≥ rent ATA ≈ 0.0015 SOL บน devnet) หรือรอ — **สิทธิ์ไม่หาย**
- SOL: ถ้ายอดหลังรับยังต่ำกว่า rent-exempt ของ wallet → fail และรอได้

### คืนเงิน booster (pool Cancelled หรือไม่มีอันดับ 1)
`refund_boost_sol` · `refund_boost_token` · `refund_boost_token_with_ata` — ใครก็กดได้ · เงินไปที่ booster เสมอ · ไม่หัก fee

### งานเก็บกวาด (crank — ใครก็กดได้ · คืน rent ให้คนที่วาง)
`collect_fee_sol` / `collect_fee_token` (fee เข้า Treasury) → `return_base_sol` / `return_base_token` (base ช่อง None กลับเกม) → `close_receipt` → `close_sol_asset` / `close_token_asset` → `close_pool` (**rent ของ MatchPool กลับ `game_authority`**) · `expire_pool` (pool ที่ไม่ settle เกิน 7 วัน)
→ ถ้าเกมอยากได้ rent คืนเร็ว ให้ Game Worker เป็น crank กดชุดนี้หลัง claim ครบ

---

## 6. Errors ที่เจอบ่อย

| code | name | เกิดเมื่อ |
|------|------|----------|
| 6000 | `NotGameAuthority` | signer ไม่ใช่ `game_authority` ของเกมนี้ |
| 6001 | `GameNotActive` | เปิด pool ตอนเกม Suspended |
| 6005 | `PoolNotOpen` | boost หลัง lock / settle |
| 6006 | `TooManyAssets` | boost ชนิดที่ 9 |
| 6007 | `MessageTooLong` | ข้อความ > 140 bytes |
| 6008 / 6031 | `BoostTooSmall` / `BoostNetBelowRentExempt` | SOL boost ต่ำเกินไป |
| 6012 | `PoolNotSettled` | claim ก่อน settle |
| 6013 | `NoWinnerAtIndex` | index ไม่ใช่ของ winner นี้ / ช่อง None |
| 6014 | `NothingToClaim` | อันดับนี้ไม่มีรางวัลใน asset นี้ |
| 6015 | `AlreadyClaimed` | claim ซ้ำ (retryable) |
| 6025 / 6026 / 6027 | `BaseMustBeFungible` / `BaseAccountsMismatch` / `PayoutBelowRentExempt` | base ไม่ถูกต้องตอน `open_pool_*` |
| 6033 | `WrongMatchPool` | `match_pool` ไม่ใช่ PDA ของ (game_program, room_id) |
| Anchor 3012 | `AccountNotInitialized` | account ที่ต้องมีอยู่ยังไม่มี (ATA ผู้ชนะ, AllowedMint, Treasury ATA ของ NFT) |
| Token 17 | `AccountFrozen` | pNFT / account ถูก freeze (ไม่รองรับ pNFT) |

รายการเต็ม 6000–6034: `idls/rewards_pool.json` (`errors`) · spec-v2 §10

---

## 7. ต้นทุน + ตัวเลข (devnet, 2026-10-10)

| อะไร | ค่า | ใครจ่าย / ได้คืน |
|------|-----|------------------|
| `MatchPool` (1,580 B) | 8,676,640 lamports (~0.0087 SOL) | `game_authority` · คืนตอน `close_pool` |
| `PoolAsset` (203 B) + vault (165–170 B) | ตาม `getMinimumBalanceForRentExemption` | base: `game_authority` · boost: booster คนแรกของ asset · คืนตอน `close_*_asset` |
| `BoostReceipt` (162 B) | — | booster · คืนตอน refund / `close_receipt` |
| platform fee | 1% ของ boost (snapshot ตอนเปิด · ≤ 20%) · NFT ไม่หัก · base ไม่หัก | เข้า Treasury |
| CU (LiteSVM) | `open_pool_sol` ผ่านเกม ≈ 34.5k · `settle` ผ่านเกม ≈ 18.9k · `boost_token` ≈ 35.9k · `claim_token` ≈ 14.4k · `claim_sol` ≈ 4.5k | — |

ตัวเลข rent ใน LiteSVM สูงกว่า devnet (อัตราเก่า) — คำนวณงบด้วยค่าจาก cluster จริง

---

## 8. ทดสอบเกมของคุณกับ rewards-pool (LiteSVM)

```bash
solana program dump -u devnet HgApfwjAg1L2FJL3fX7A4WX5yfhrVV8k9Ahii5Pn8HVW tests/fixtures/rewards_pool.so
```
- โหลด `rewards_pool.so` + game program ของคุณใน LiteSVM 0.17 (`svm.add_program`)
- account ของ platform-core (`GameEntry`, `PlatformConfig`, `Treasury`, `AllowedMint`) **เขียนตรงๆ ได้** (owner = platform-core · layout จาก `platform-core/idls/platform_core.json`) — rewards-pool แค่อ่าน ไม่ CPI ไป platform-core · ตัวอย่าง: [`programs/rewards_pool/tests/common/mod.rs`](programs/rewards_pool/tests/common/mod.rs) (`put_game_entry`, `put_config`, `allow_mint`)
- เติม SOL ให้ `game_authority` ด้วย `svm.airdrop`
- ตัวอย่าง flow ครบ (open → boost → settle → claim → ปิด): [`programs/rewards_pool/tests/test_flow.rs`](programs/rewards_pool/tests/test_flow.rs)

---

## 9. เวอร์ชัน · ติดต่อ

- IDL มี version (`metadata.version`) · การเปลี่ยน layout / seed / error = version ใหม่ + ประกาศ · error / enum / seed เป็น append-only
- upgrade program → relayer จะตอบ 503 `program_changed` จนกว่า deploy slot ใหม่ถูกอัปเดต (แจ้ง solana-poc)
- ยังไม่มี: Random reward, Game Reward Vault (R17) · NFT แบบ Metaplex Core / compressed (รอ allowlist ชนิดใหม่ของ platform-core)
- คำถาม / ขอเปลี่ยน interface: Arra thread `channel:rewards-pool` (#2) หรือ `<internal-note>` ของ `maintainer`
