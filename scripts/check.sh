#!/bin/sh
# The one check (AGENTS.md): what .github/workflows/ci.yml runs that needs no secrets,
# display or network service. Rockhound has tests (Rust unit/integration, frontend vitest).
# Left out, because CI runs them: the llvm-cov coverage gate, the e2e WebDriver run
# (needs a display), cargo/npm audit (network advisories) and the Windows build/installer.
# The Tauri crate (app/src-tauri) needs webkit2gtk, so it is linted and tested only when
# ROCKHOUND_CHECK_FULL=1 (CI sets it); otherwise the three pure-Rust crates are checked.
set -eu
cd "$(dirname "$0")/.."

n=$(wc -l < AGENTS.md)
echo "AGENTS.md: $n lines"
[ "$n" -le 20 ] || { echo "check: AGENTS.md has $n lines; keep it at 20 or fewer" >&2; exit 1; }

for tool in cargo node npm; do
  command -v "$tool" >/dev/null 2>&1 || { echo "check: missing tool: $tool" >&2; exit 1; }
done

if [ "${ROCKHOUND_CHECK_FULL:-}" = 1 ]; then
  SCOPE="--workspace"
else
  SCOPE="-p rockhound-core -p rockhound-replay -p rockhound-sigextract"
fi

cargo fmt --all --check
# shellcheck disable=SC2086
cargo clippy $SCOPE --all-targets -- -D warnings
# shellcheck disable=SC2086
cargo test $SCOPE

(cd app && npm ci && npm run build && npm run typecheck:e2e && npm test)
# A version bump must come with its DEVLOG entry (it becomes the release notes).
node app/scripts/release.mjs notes "$(node app/scripts/release.mjs version)" > /dev/null
echo "check: OK"
