#!/usr/bin/env bash
# Full test run. Tests load PRE-BUILT .so files, so always build first.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.local/share/solana/install/active_release/bin:$HOME/.cargo/bin:$PATH"
[ -f tests/fixtures/mpl_token_metadata.so ] || ./scripts/fetch-fixtures.sh
anchor build
# mock-game reads idls/rewards_pool.json via declare_program! → refresh it from the build.
cp target/idl/rewards_pool.json idls/rewards_pool.json
cargo build-sbf --manifest-path tests/mock-game/Cargo.toml --sbf-out-dir target/deploy
cargo test -p rewards_pool "$@"
