#!/bin/sh
# What an agent must see green before "done" (AGENTS.md): the steps of .github/workflows/ci.yml
# that need no Tauri system libraries, display or network service.
# Left out: the app/src-tauri crate (needs webkit2gtk), cargo-llvm-cov coverage gate,
# e2e (WebDriver), cargo/npm audit, and the Windows build. CI still runs them.
set -eu
cd "$(dirname "$0")/.."
n=$(wc -l < AGENTS.md); echo "AGENTS.md: $n lines"; [ "$n" -le 20 ]
cargo fmt --all --check
CRATES="-p rockhound-core -p rockhound-replay -p rockhound-sigextract"
cargo clippy $CRATES --all-targets -- -D warnings
cargo test $CRATES
if command -v npm >/dev/null 2>&1; then
  (cd app && npm ci && npm run build && npm run typecheck:e2e && npm test)
  node app/scripts/release.mjs notes "$(node app/scripts/release.mjs version)" > /dev/null
else
  echo "check: npm not found, frontend steps skipped"
fi
echo "check: OK"
