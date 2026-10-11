# rewards-pool

An Anchor program for **blockchain-games-hub** that holds and pays out the rewards of each match:

- Games CPI into `open_pool*` / `lock_boosts` / `settle` / `cancel` (authenticated by the game's PDA signer `[b"game_authority"]`)
- **Base reward** — the game sets a fixed amount per rank (SOL or an allowlisted token)
- **Boost** — anyone can add SOL / tokens / NFTs (allowlisted mints) to an open pool, with a message · **all boosts go to rank 1**
- **settle → claim** — `settle` records the result, `claim` transfers one (winner, asset) at a time · no signer required (anyone can push it; the destination is pinned) · no deadline
- Platform fee 1% (configurable, ≤ 20%) on fungible boosts only · no fee on NFTs · no fee on the base reward

**Integrating your game with rewards-pool → read [`SKILL.md`](SKILL.md)**

| | |
|---|---|
| Program (devnet) | `HgApfwjAg1L2FJL3fX7A4WX5yfhrVV8k9Ahii5Pn8HVW` · deploy slot 509454201 |
| IDL | [`idls/rewards_pool.json`](idls/rewards_pool.json) v1.0.0 |
| Spec | [`specs/spec-v2.md`](specs/spec-v2.md) (full contract · written in Thai) |
| Example game | [`tests/mock-game/src/lib.rs`](tests/mock-game/src/lib.rs) (test only — do not deploy) |

## Build / test

```bash
./scripts/fetch-fixtures.sh   # dumps the Token Metadata program from devnet for tests — run once
./scripts/test.sh             # anchor build + mock-game + cargo test (41 tests · LiteSVM 0.17)
```
Anchor 1.2.1 · solana-cli 4.3.0 · Rust 1.99.0 (`rust-toolchain.toml`)

## Status

**v1 · deployed on devnet (2026-10-10)** — devnet only

## Contact

Questions / interface change requests: open an issue in this repository or contact the platform Admin.
