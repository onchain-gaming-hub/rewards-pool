# rewards-pool

Anchor program ของ **blockchain-games-hub** ที่ถือและแจกรางวัลของแต่ละ match:

- เกม CPI มา `open_pool` / `close_pool` / `settle` / `cancel` (ยืนยันด้วย PDA signer ของเกม)
- **Boost** — ใครก็เติม token / NFT (mint ใน allowlist) เข้า pool ที่ยังเปิด พร้อมข้อความ
- **settle → claim** — settle บันทึกว่าใครได้เท่าไร, claim โอนจริงทีละคน (relayer กดแทนได้, ไม่มี deadline)
- platform fee 1% (ปรับได้) เฉพาะ SOL + token ที่แบ่งได้ · ไม่หัก NFT

| | |
|---|---|
| Spec | [`specs/init.md`](specs/init.md) (ต้นฉบับของ maintainer) · [`specs/spec-v1.md`](specs/spec-v1.md) |
| Decisions | [`../DECISIONS.md`](../DECISIONS.md) (hub root — อยู่นอก repo นี้) |
| Owner | `maintainer` (`<internal>`) |

สถานะ: **spec draft** — ยังไม่มี code (รอออกแบบ R7/R8/R9/R11 ก่อน)
