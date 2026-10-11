# rewards-pool

Anchor program ของ **blockchain-games-hub** ที่ถือและแจกรางวัลของแต่ละ match:

- เกม CPI มา `open_pool*` / `lock_boosts` / `settle` / `cancel` (ยืนยันด้วย PDA signer `[b"game_authority"]` ของเกม)
- **รางวัลพื้นฐาน** — เกมกำหนดจำนวนตายตัวต่ออันดับ (SOL หรือ token ใน allowlist)
- **Boost** — ใครก็เติม SOL / token / NFT (mint ใน allowlist) เข้า pool ที่ยังเปิด พร้อมข้อความ · **ทั้งหมดไปที่อันดับ 1**
- **settle → claim** — settle บันทึกผล, claim โอนจริงทีละ (ผู้ชนะ, asset) · ไม่ต้องมี signer (ใครก็ดันให้ได้ ปลายทางล็อกไว้) · ไม่มี deadline
- platform fee 1% (ปรับได้ ≤ 20%) เฉพาะ boost ที่แบ่งได้ · ไม่หัก NFT · ไม่หักรางวัลพื้นฐาน

**ต่อเกมของคุณเข้ากับ rewards-pool → อ่าน [`SKILL.md`](SKILL.md)**

| | |
|---|---|
| Program (devnet) | `HgApfwjAg1L2FJL3fX7A4WX5yfhrVV8k9Ahii5Pn8HVW` · deploy slot 509454201 · [`specs/deploy/devnet-runbook.md`](specs/deploy/devnet-runbook.md) |
| IDL | [`idls/rewards_pool.json`](idls/rewards_pool.json) v1.0.0 |
| Spec | [`specs/spec-v2.md`](specs/spec-v2.md) (ฉบับใช้งาน) · [`specs/init.md`](specs/init.md) (ต้นฉบับของ maintainer) · [`specs/spec-v1.md`](specs/spec-v1.md) (เก่า) · [`specs/designs/`](specs/designs/) (ที่มาของเหตุผล) |
| Decisions | [`../DECISIONS.md`](../DECISIONS.md) (hub root — อยู่นอก repo นี้) · R1–R33 |
| Owner | `maintainer` (`<internal>`) |

## Build / test

```bash
./scripts/fetch-fixtures.sh   # Token Metadata program (devnet dump) สำหรับ test — ครั้งแรกครั้งเดียว
./scripts/test.sh             # anchor build + mock-game + cargo test (41 tests · LiteSVM 0.17)
```
Anchor 1.2.1 · solana-cli 4.3.0 · Rust 1.99.0 (`rust-toolchain.toml`) · `tests/mock-game` = TEST ONLY (ห้าม deploy)

สถานะ: **v1 · deploy devnet แล้ว (2026-10-10)**
