#!/usr/bin/env bash
# DB1 acceptance check: a release build must contain ZERO debug surface.
#
# Two layers (see the DB1 card in docs/capability-cards.md):
#   1. Compile-level: the debug module (src/systems/debug.rs) and every debug
#      registration in DebugPlugin are #[cfg(debug_assertions)]-gated, so any
#      leak of debug code into a release path is a *compile error* — this
#      script's `cargo build --release` step fails on such a leak.
#   2. Binary-level: unique debug-only strings (egui panel title, cheat/log
#      prefixes) must be absent from the built binary. This proves the code
#      paths were cfg-stripped, not just unreachable.
#
# Usage: scripts/check_release_debug_surface.sh   (run from games/warden)

set -euo pipefail
cd "$(dirname "$0")/.."

echo "[1/2] cargo build --release (compile-level gate: ungated debug refs fail here)"
cargo build --release

BIN="target/release/warden"
if [[ ! -f "$BIN" ]]; then
    echo "FAIL: release binary not found at $BIN" >&2
    exit 1
fi

echo "[2/2] scanning $BIN for debug-only strings (binary-level gate)"
FAILED=0
# Strings that exist ONLY inside #[cfg(debug_assertions)] code paths.
DEBUG_STRINGS=(
    "Warden Debug (F1)"          # DB1 egui panel title
    "run marked as debug"        # DB1 debug-run marking log
    "instant lose"               # DB1 cheat log
    "run restarted"              # DB1 restart log
    "scenario '{}' loaded"       # DB2 scenario loader log
    "screenshot saved"           # F12 screenshot log
    "dash] fps"                  # log dashboard prefix
    "[db3]"                      # DB3 time/overlay log + HUD strip prefix
    "[sm]"                       # SM1 showroom log prefix
)
for s in "${DEBUG_STRINGS[@]}"; do
    if LC_ALL=C grep -aqF -- "$s" "$BIN"; then
        echo "FAIL: debug-only string leaked into the release binary: '$s'" >&2
        FAILED=1
    fi
done

if [[ "$FAILED" -ne 0 ]]; then
    exit 1
fi
echo "OK: release build has zero debug surface (compile + binary level)"
