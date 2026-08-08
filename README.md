<p align="center">
  <h1 align="center">tokemon</h1>
  <p align="center">
    an LLM <b>tok</b>en <b>mon</b>itor
  </p>
  <p align="center">
    <a href="https://crates.io/crates/tokemon"><img alt="crates.io" src="https://img.shields.io/crates/v/tokemon.svg"></a>
    <a href="https://opensource.org/licenses/MIT"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
    <a href="https://www.rust-lang.org/"><img alt="Built with Rust" src="https://img.shields.io/badge/built%20with-Rust-orange.svg"></a>
    <img alt="16 providers" src="https://img.shields.io/badge/providers-16-green.svg">
  </p>
</p>

---

Unified token usage tracking across all your AI coding tools. `tokemon top` provides a live, continuously updating dashboard of your token usage and costs across 16 different providers. It reads local session logs, estimates costs via LiteLLM pricing, and presents beautiful TUI monitoring, static reports, or raw JSON.

![tokemon dashboard](assets/dashboard.png)

## Highlights

- **16 providers** — Claude Code, Codex, Gemini CLI, Amp, OpenCode, Cline, Roo Code, Kilo Code, Copilot, Pi Agent, Kimi, Droid, OpenClaw, Qwen Code, Piebald, Cursor
- **Auto-discovery** — detects which tools are installed and finds their log directories automatically
- **Cost estimation** — LiteLLM pricing database with three-level model name matching
- **SQLite cache** — parsed data is cached for instant repeated runs and survives log rotation
- **Budget pacemaker** — set daily/weekly/monthly spending limits with progress tracking
- **Statusline mode** — compact one-line output for shell prompts and status bars (`tokemon statusline`)
- **Session breakdown** — per-session cost analysis across all providers (`tokemon sessions`)
- **MCP server** — expose usage data to AI tools via Model Context Protocol (`tokemon mcp`)
- **Two display modes** — compact one-row-per-day (default) or detailed per-model breakdown with responsive API Provider and Client columns
- **Filtering** — by provider (`-p`), date range (`--since` / `--until`), frequency (`-f`), sort order (`-o`)
- **JSON & CSV output** — `--json` or `--csv` for piping to `jq` or downstream tools
- **Parallel parsing** — multi-threaded file processing with [rayon](https://github.com/rayon-rs/rayon)
- **Configurable** — persistent preferences via `~/.config/tokemon/config.toml`
- **Extensible** — adding a new source is ~20 lines of Rust

## Installation

### From crates.io (recommended)

```bash
cargo install tokemon              # latest stable
cargo install tokemon --version 0.2.0   # specific version
```

### Via Homebrew (macOS / Linux)

```bash
brew install mm65x/tap/tokemon
```

### Pre-built binaries

Download from [GitHub Releases](https://github.com/mm65x/tokemon/releases) for Linux (x86/ARM), macOS (Intel/Apple Silicon), and Windows.

Or use [cargo-binstall](https://github.com/cargo-bins/cargo-binstall) for automatic binary downloads:

```bash
cargo binstall tokemon             # latest stable
cargo binstall tokemon@0.2.0       # specific version
```

### From source

Requires [Rust 1.83+](https://rustup.rs/).

```bash
git clone https://github.com/mm65x/tokemon.git
cd tokemon
cargo install --path .
```

## Quick Start

```bash
# Launch the live monitoring dashboard (default view: today)
tokemon top

# See which providers are installed
tokemon discover

# Daily static usage report
tokemon report

# Per-model breakdown view
tokemon report -d breakdown

# Weekly or monthly report
tokemon report -f weekly
tokemon report -f monthly --json

# Budget overview
tokemon budget

# Per-session cost breakdown
tokemon sessions

# Statusline for shell prompts
tokemon statusline
# $42.17 | 1.2B tok | 1 provider | today

tokemon statusline -f weekly
# $312.50 | 8.4B tok | 2 providers | this week
```

## Usage

```
tokemon [OPTIONS] <COMMAND>

Commands:
  top          Live monitoring dashboard
  report       Generate a static usage report (table, json, or csv)
  statusline   Compact one-line output for shell prompts and status bars
  budget       Show spending vs configured limits
  sessions     Show per-session cost breakdown
  discover     List auto-detected providers
  init         Generate default config file
  prune        Delete old preserved data from the cache
  mcp          Start MCP (Model Context Protocol) server over stdio

Options:
  -f, --frequency <FREQ>  daily (default), weekly, or monthly
  -d, --display <MODE>    compact (default) or breakdown
  -p, --provider <NAME>   Filter by provider (repeatable)
      --since <DATE>      Start date (YYYY-MM-DD)
      --until <DATE>      End date (YYYY-MM-DD)
      --no-cost           Skip cost calculation
      --offline           Use cached pricing only
      --refresh           Force re-discovery of files
      --reparse           Force re-parse of all files from disk
  -o, --order <ORDER>     asc (default) or desc
      --json              Output as JSON
      --csv               Output as CSV
```

## Configuration

```bash
tokemon init
# Creates ~/.config/tokemon/config.toml
```

```toml
default_format = "table"
breakdown = false
no_cost = false
offline = false
sort_order = "asc"
providers = []

[budget]
daily = 50.0      # $50/day limit
weekly = 250.0    # $250/week limit
monthly = 800.0   # $800/month limit

[columns]
date = true
model = true
api_provider = true
client = true
input = true
output = true
cache_write = true
cache_read = true
total_tokens = true
cost = true
```

CLI flags always override config values.

## Supported Providers

| Provider | Log Location | Format |
|----------|-------------|--------|
| Claude Code | `~/.claude/projects/{project}/{uuid}.jsonl` | JSONL |
| Codex CLI | `~/.codex/sessions/YYYY/MM/DD/*.jsonl` | JSONL |
| Gemini CLI | `~/.gemini/tmp/{project}/chats/session-*.json` | JSON |
| Amp | `~/.local/share/amp/threads/` | JSONL |
| OpenCode | `~/.local/share/opencode/opencode.db` | SQLite |
| Cline | VSCode globalStorage | JSON |
| Roo Code | VSCode globalStorage | JSON |
| Kilo Code | VSCode globalStorage | JSON |
| Copilot | VSCode workspaceStorage | JSON (stub) |
| Cursor | `~/.config/tokscale/cursor-cache/usage*.csv` | CSV |
| Qwen Code | `~/.qwen/tmp/{project}/session.json` | JSON |
| Pi Agent | `~/.pi/agent/sessions/{project}/*.jsonl` | JSONL |
| Kimi | `~/.kimi/sessions/` | JSONL |
| Droid | `~/.factory/sessions/` | JSONL |
| OpenClaw | `~/.openclaw/sessions/` | JSONL |
| Piebald | `~/Library/Application Support/piebald/app.db` | SQLite (stub) |

Adding a new source requires implementing the `Source` trait — see `src/source/jsonl_source.rs` for a template that covers most JSONL-based tools in ~20 lines.

## Development

```bash
make help          # Show available targets
make build         # Build release binary
make test          # Run tests
make lint          # Run clippy
make fmt           # Format code
make ci            # Run all checks (fmt + lint + test)
```

## Architecture

```
src/
├── main.rs              # CLI entry and command dispatch
├── lib.rs               # Library entry point
├── cli.rs               # clap argument definitions
├── config.rs            # TOML config loading and validation
├── types.rs             # Core data types (Record, ModelUsage, etc.)
├── error.rs             # Error types
├── cache.rs             # SQLite cache layer
├── pipeline.rs          # Shared data loading orchestration (used by CLI and MCP)
├── display.rs           # Name translation (client, model, API provider)
├── pacemaker.rs         # Budget tracking and limits
├── timestamp.rs         # Shared timestamp parsing
├── cost.rs              # Pricing engine
├── rollup.rs            # Daily/weekly/monthly grouping
├── dedup.rs             # Hash-based deduplication
├── render/              # Table, CSV, and JSON rendering
├── tui/                 # Terminal UI dashboard (`tokemon top`)
│   ├── app.rs           # Core state and event loop
│   ├── watcher.rs       # Background file modification watcher
│   ├── settings_state.rs# Configuration settings state
│   ├── sparkline_data.rs# Rendering sparklines
│   ├── theme.rs         # TUI color palette
│   └── widgets/         # TUI components (usage table, summary cards)
├── mcp.rs               # MCP server (Model Context Protocol)
├── paths.rs             # Platform-specific path resolution
└── source/
    ├── mod.rs            # Source trait and SourceSet
    ├── discover.rs       # Bounded read_dir file discovery utilities
    ├── jsonl_source.rs   # Generic JSONL source
    ├── cline_format.rs   # Shared Cline-format parser
    └── ...               # One file per provider
```

## License

MIT
