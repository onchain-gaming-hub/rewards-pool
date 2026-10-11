#!/usr/bin/env bash
# Test fixtures that are third-party program binaries (not committed).
# mpl_token_metadata.so: Token Metadata program dumped from devnet.
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
mkdir -p tests/fixtures
solana program dump -u devnet metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s tests/fixtures/mpl_token_metadata.so
shasum -a 256 tests/fixtures/mpl_token_metadata.so   # 2026-10-10: bb0842f6d5740eab…
