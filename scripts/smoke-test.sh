#!/usr/bin/env bash
# End-to-end CLI and MCP smoke tests executed entirely inside an isolated
# temporary TOKEMON_HOME sandbox.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

if [[ -n "${TOKEMON_BIN:-}" ]]; then
    BIN="$TOKEMON_BIN"
else
    cargo build --quiet
    BIN="$ROOT_DIR/target/debug/tokemon"
fi

if [[ ! -x "$BIN" ]]; then
    echo "Error: tokemon binary not executable at $BIN" >&2
    exit 1
fi

SANDBOX_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tokemon-smoke.XXXXXX")"
cleanup() {
    rm -rf "$SANDBOX_DIR"
}
trap cleanup EXIT

"$ROOT_DIR/scripts/sandbox.sh" seed "$SANDBOX_DIR" >/dev/null

run_sandboxed() {
    env -u TOKEMON_CACHE_DIR -u TOKEMON_CONFIG_DIR TOKEMON_HOME="$SANDBOX_DIR" "$BIN" "$@"
}

echo "=== Tokemon Smoke Tests ==="
echo "Binary:  $BIN"
echo "Sandbox: $SANDBOX_DIR"
echo ""

echo "[1/11] --version"
"$BIN" --version

echo "[2/11] discover"
DISCOVER_OUT="$(run_sandboxed discover)"
for expected in "Claude Code" "Codex CLI" "Gemini CLI" "Continue"; do
    if ! grep -qF "$expected" <<<"$DISCOVER_OUT"; then
        echo "FAIL: discover missing expected provider '$expected'" >&2
        echo "$DISCOVER_OUT" >&2
        exit 1
    fi
done

echo "[3/11] report (compact table)"
REPORT_TABLE="$(run_sandboxed report --offline)"
grep -qF "TOTAL" <<<"$REPORT_TABLE"

echo "[4/11] report -d breakdown"
REPORT_BREAKDOWN="$(run_sandboxed report -d breakdown --offline)"
grep -qF "TOTAL" <<<"$REPORT_BREAKDOWN"

echo "[5/11] report -f weekly --json"
run_sandboxed report -f weekly --offline --json | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["period"] == "weekly", data
assert data["total_tokens"] > 0, data
assert len(data["summaries"]) >= 4, len(data["summaries"])
assert set(data["providers_found"]) >= {"claude-code", "codex", "gemini", "continue"}, data["providers_found"]
'

echo "[6/11] report -f monthly --csv"
REPORT_CSV="$(run_sandboxed report -f monthly --offline --csv)"
grep -qE "^date,|^[0-9]{4}-" <<<"$REPORT_CSV"

echo "[7/11] status"
run_sandboxed status --offline | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["schema_version"] == 1, data
assert data["state"] == "populated", data
assert data["total_tokens"] > 0, data
assert data["total_cost"] > 0, data
assert data["capabilities"]["cost"] is True, data
'

echo "[8/11] statusline"
STATUSLINE_OUT="$(run_sandboxed statusline --offline)"
grep -qF "|" <<<"$STATUSLINE_OUT"

echo "[9/11] budget"
BUDGET_OUT="$(run_sandboxed budget --offline)"
grep -qF "Daily:" <<<"$BUDGET_OUT"

echo "[10/11] sessions --json & prune"
run_sandboxed sessions --offline --json | python3 -c '
import json, sys
data = json.load(sys.stdin)
assert data["total_tokens"] > 0, data
assert len(data["sessions"]) > 0, data
'
run_sandboxed prune --before 2020-01-01 >/dev/null

echo "[11/11] mcp integration suite"
env -u TOKEMON_CACHE_DIR -u TOKEMON_CONFIG_DIR TOKEMON_HOME="$SANDBOX_DIR" \
    "$ROOT_DIR/tests/test_mcp.sh" "$BIN"

echo ""
echo "ALL SMOKE TESTS PASSED"
