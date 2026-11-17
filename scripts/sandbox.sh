#!/usr/bin/env bash
# Isolated local sandbox helper for testing tokemon without touching real
# user config, cache, or provider logs.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
    cat <<'EOF'
Usage:
  scripts/sandbox.sh seed <dir>           Seed a 120+ day multi-provider dataset
  scripts/sandbox.sh run [args...]        Run tokemon against a fresh seeded sandbox
  scripts/sandbox.sh top [args...]        Launch `tokemon top --offline` in a seeded sandbox
  scripts/sandbox.sh live-top [args...]   Launch `tokemon top` against real logs with scratch cache/config

Environment:
  TOKEMON_BIN    Path to tokemon binary (defaults to target/release/tokemon, then target/debug/tokemon)
  KEEP_SANDBOX   Set to 1 to keep the temporary sandbox directory after `run` / `top` / `live-top`
EOF
}

resolve_bin() {
    if [[ -n "${TOKEMON_BIN:-}" ]]; then
        if [[ -x "$TOKEMON_BIN" ]]; then
            echo "$TOKEMON_BIN"
            return 0
        fi
        echo "Error: TOKEMON_BIN is not executable: $TOKEMON_BIN" >&2
        exit 1
    fi

    if [[ -x "$ROOT_DIR/target/release/tokemon" ]]; then
        echo "$ROOT_DIR/target/release/tokemon"
        return 0
    fi

    if [[ -x "$ROOT_DIR/target/debug/tokemon" ]]; then
        echo "$ROOT_DIR/target/debug/tokemon"
        return 0
    fi

    echo "[sandbox] Building debug binary..." >&2
    (cd "$ROOT_DIR" && cargo build --quiet)
    echo "$ROOT_DIR/target/debug/tokemon"
}

seed_dir() {
    local target_dir="$1"
    mkdir -p "$target_dir"

    python3 - "$target_dir" <<'PY'
import json
import os
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

home = Path(sys.argv[1])
now = datetime.now(timezone.utc).replace(second=0, microsecond=0)

# Span today, this week, this month, and 120+ days back so history scrolling
# and All-Time aggregation can be exercised thoroughly.
offsets_days = [0, 0, 1, 2, 4, 8, 14, 22, 31, 45, 63, 82, 98, 114, 126]

claude_dir = home / ".claude" / "projects" / "sandbox-repo"
claude_dir.mkdir(parents=True, exist_ok=True)

claude_sessions = {}
for idx, days_ago in enumerate(offsets_days):
    ts = now - timedelta(days=days_ago, hours=(idx % 6) + 1)
    if days_ago == 0 and idx == 0:
        ts = now - timedelta(minutes=15)
    model = "claude-opus-4-1-20250805" if idx % 3 == 0 else "claude-sonnet-4-20250514"
    sess_name = f"sess-claude-{idx // 2:02d}.jsonl"
    entry = {
        "type": "assistant",
        "timestamp": ts.strftime("%Y-%m-%dT%H:%M:%S.000Z"),
        "requestId": f"req_sb_{idx:03d}",
        "message": {
            "model": model,
            "id": f"msg_sb_{idx:03d}",
            "usage": {
                "input_tokens": 1200 + idx * 150,
                "output_tokens": 450 + idx * 60,
                "cache_creation_input_tokens": 800 + idx * 80,
                "cache_read_input_tokens": 3000 + idx * 250,
            },
        },
    }
    claude_sessions.setdefault(sess_name, []).append(json.dumps(entry))

for fname, lines in claude_sessions.items():
    (claude_dir / fname).write_text("\n".join(lines) + "\n", encoding="utf-8")

# Seed Codex CLI logs under .codex/sessions/YYYY/MM/DD/rollout-*.jsonl
for idx, days_ago in enumerate([0, 3, 18, 52, 95, 122]):
    ts = now - timedelta(days=days_ago, hours=2)
    day_dir = (
        home
        / ".codex"
        / "sessions"
        / f"{ts.year:04d}"
        / f"{ts.month:02d}"
        / f"{ts.day:02d}"
    )
    day_dir.mkdir(parents=True, exist_ok=True)
    turn = {
        "type": "turn_context",
        "timestamp": ts.strftime("%Y-%m-%dT%H:%M:%S.000Z"),
        "payload": {"model": "gpt-5-codex"},
    }
    evt = {
        "type": "event_msg",
        "timestamp": (ts + timedelta(seconds=12)).strftime("%Y-%m-%dT%H:%M:%S.000Z"),
        "payload": {
            "type": "token_count",
            "info": {
                "last_token_usage": {
                    "input_tokens": 2200 + idx * 300,
                    "output_tokens": 600 + idx * 90,
                    "cached_input_tokens": 400 + idx * 50,
                }
            },
        },
    }
    (day_dir / f"rollout-sb-{idx:02d}.jsonl").write_text(
        json.dumps(turn) + "\n" + json.dumps(evt) + "\n",
        encoding="utf-8",
    )

# Seed Gemini CLI logs under .gemini/tmp/<project>/chats/session-*.json
gemini_chats = home / ".gemini" / "tmp" / "sandbox-repo" / "chats"
gemini_chats.mkdir(parents=True, exist_ok=True)
for idx, days_ago in enumerate([0, 6, 35, 88, 120]):
    ts = now - timedelta(days=days_ago, hours=3)
    payload = {
        "messages": [
            {
                "type": "gemini",
                "model": "gemini-2.5-flash",
                "timestamp": ts.strftime("%Y-%m-%dT%H:%M:%S.000Z"),
                "tokens": {
                    "input": 950 + idx * 140,
                    "output": 320 + idx * 45,
                    "cached": 180 + idx * 30,
                    "thoughts": 90 + idx * 15,
                },
            }
        ]
    }
    (gemini_chats / f"session-sb-{idx:02d}.json").write_text(
        json.dumps(payload, indent=2) + "\n",
        encoding="utf-8",
    )

# Seed Continue logs under .continue/dev_data/<version>/tokensGenerated.jsonl
continue_dir = home / ".continue" / "dev_data" / "0.2.0"
continue_dir.mkdir(parents=True, exist_ok=True)
continue_lines = []
for idx, days_ago in enumerate([1, 12, 48, 110]):
    ts = now - timedelta(days=days_ago, hours=4)
    continue_lines.append(
        json.dumps(
            {
                "timestamp": ts.strftime("%Y-%m-%dT%H:%M:%S.000Z"),
                "eventName": "tokensGenerated",
                "schema": "0.2.0",
                "model": "claude-sonnet-4",
                "provider": "anthropic",
                "promptTokens": 1400 + idx * 200,
                "generatedTokens": 380 + idx * 50,
            }
        )
    )
(continue_dir / "tokensGenerated.jsonl").write_text(
    "\n".join(continue_lines) + "\n", encoding="utf-8"
)

# Seed offline pricing cache inside TOKEMON_HOME/.cache/tokemon/pricing.json
cache_dir = home / ".cache" / "tokemon"
cache_dir.mkdir(parents=True, exist_ok=True)
pricing = {
    "claude-sonnet-4-20250514": {
        "input_cost_per_token": 0.000003,
        "output_cost_per_token": 0.000015,
        "cache_read_input_token_cost": 0.0000003,
        "cache_creation_input_token_cost": 0.00000375,
    },
    "anthropic/claude-sonnet-4": {
        "input_cost_per_token": 0.000003,
        "output_cost_per_token": 0.000015,
        "cache_read_input_token_cost": 0.0000003,
        "cache_creation_input_token_cost": 0.00000375,
    },
    "claude-opus-4-1-20250805": {
        "input_cost_per_token": 0.000015,
        "output_cost_per_token": 0.000075,
        "cache_read_input_token_cost": 0.0000015,
        "cache_creation_input_token_cost": 0.00001875,
    },
    "gpt-5-codex": {
        "input_cost_per_token": 0.000002,
        "output_cost_per_token": 0.00001,
        "cache_read_input_token_cost": 0.0000005,
    },
    "gemini-2.5-flash": {
        "input_cost_per_token": 0.0000003,
        "output_cost_per_token": 0.0000025,
        "cache_read_input_token_cost": 0.000000075,
    },
}
(cache_dir / "pricing.json").write_text(json.dumps(pricing, indent=2), encoding="utf-8")

# Seed isolated config with budgets so budget and statusline work offline
config_dir = home / ".config" / "tokemon"
config_dir.mkdir(parents=True, exist_ok=True)
(config_dir / "config.toml").write_text(
    """default_format = "table"
breakdown = false
no_cost = false
offline = true
sort_order = "asc"
providers = []

[budget]
daily = 25.0
weekly = 100.0
monthly = 350.0
""",
    encoding="utf-8",
)
PY
}

cleanup_dir() {
    local dir="$1"
    if [[ "${KEEP_SANDBOX:-0}" == "1" ]]; then
        echo "[sandbox] Kept sandbox directory: $dir" >&2
    else
        rm -rf "$dir"
    fi
}

cmd="${1:-}"
if [[ -z "$cmd" ]]; then
    usage
    exit 1
fi
shift

case "$cmd" in
    seed)
        if [[ $# -lt 1 ]]; then
            echo "Error: seed requires a target directory" >&2
            usage
            exit 1
        fi
        seed_dir "$1"
        echo "Seeded sandbox at: $1"
        ;;
    run)
        BIN="$(resolve_bin)"
        SANDBOX_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tokemon-sandbox.XXXXXX")"
        trap 'cleanup_dir "$SANDBOX_DIR"' EXIT
        seed_dir "$SANDBOX_DIR"
        env -u TOKEMON_CACHE_DIR -u TOKEMON_CONFIG_DIR TOKEMON_HOME="$SANDBOX_DIR" "$BIN" "$@"
        ;;
    top)
        BIN="$(resolve_bin)"
        SANDBOX_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tokemon-sandbox.XXXXXX")"
        trap 'cleanup_dir "$SANDBOX_DIR"' EXIT
        seed_dir "$SANDBOX_DIR"
        env -u TOKEMON_CACHE_DIR -u TOKEMON_CONFIG_DIR TOKEMON_HOME="$SANDBOX_DIR" "$BIN" top --offline "$@"
        ;;
    live-top)
        BIN="$(resolve_bin)"
        SCRATCH_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tokemon-live-scratch.XXXXXX")"
        trap 'cleanup_dir "$SCRATCH_DIR"' EXIT
        CACHE_SCRATCH="$SCRATCH_DIR/cache"
        CONFIG_SCRATCH="$SCRATCH_DIR/config"
        mkdir -p "$CACHE_SCRATCH" "$CONFIG_SCRATCH"

        for real_cache in "$HOME/Library/Caches/tokemon" "$HOME/.cache/tokemon"; do
            if [[ -d "$real_cache" ]]; then
                cp -R "$real_cache/." "$CACHE_SCRATCH/" 2>/dev/null || true
                break
            fi
        done

        for real_config in "$HOME/Library/Application Support/tokemon" "$HOME/.config/tokemon"; do
            if [[ -d "$real_config" ]]; then
                cp -R "$real_config/." "$CONFIG_SCRATCH/" 2>/dev/null || true
                break
            fi
        done

        env -u TOKEMON_HOME TOKEMON_CACHE_DIR="$CACHE_SCRATCH" TOKEMON_CONFIG_DIR="$CONFIG_SCRATCH" "$BIN" top "$@"
        ;;
    -h|--help|help)
        usage
        ;;
    *)
        echo "Unknown subcommand: $cmd" >&2
        usage
        exit 1
        ;;
esac
