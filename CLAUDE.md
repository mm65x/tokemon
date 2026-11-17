# CLAUDE.md

## Build & Test

Build from the symlinked directory (`~/code/tokenusage`), which avoids OneDrive filesystem hook issues:

```bash
make setup-hooks                 # enable .githooks/pre-push and script permissions
make check                       # fmt-check + pedantic clippy + cargo test + swift test + smoke tests
make dev-install                 # build release binary and install to ~/.cargo/bin/tokemon (codesigned on macOS)
```

Direct Cargo equivalents:

```bash
cargo build --release            # debug: cargo build
cargo test                       # run all Rust unit & integration tests
swift test --package-path macos  # run macOS menu-bar tests (macOS only)
```

## Isolated Local Testing (Sandbox)

`src/paths.rs` supports environment variable overrides so local testing never touches real user logs, SQLite cache, or config:

- `TOKEMON_HOME` — overrides the base home directory (provider logs, `.cache/tokemon`, and `.config/tokemon`)
- `TOKEMON_CACHE_DIR` — overrides only the SQLite & pricing cache directory
- `TOKEMON_CONFIG_DIR` — overrides only the config directory

Helper targets:

```bash
make sandbox-top                 # launch `tokemon top --offline` in a seeded 120+ day multi-provider sandbox
make dev-top                     # launch `tokemon top` reading real logs with isolated scratch cache/config
make smoke                       # run isolated CLI + MCP end-to-end smoke tests (`./scripts/smoke-test.sh`)
```

## Lint

```bash
cargo clippy -- -W clippy::pedantic -A clippy::module_name_repetitions -D warnings
cargo fmt -- --check
```

Note: `make lint` and CI (`.github/workflows/ci.yml`) enforce the flags above.

## Git

- **Remote**: `origin` (`https://github.com/mm65x/tokemon.git` via `gh` or `git@github-mm65x:mm65x/tokemon.git` via SSH)
- **Identity**: `mm65x <mm65x@users.noreply.github.com>` (set in local git config)

### Branching

- **`master`** — stable releases only. Tagged with `vX.Y.Z`. Protected branch (direct pushes blocked by `.githooks/pre-push`).
- **`develop`** — main development branch. All work merges here first.
- **Feature/fix branches** — branch off `develop`, merge back via PR.

### Workflow

1. Create a feature branch from `develop`: `git checkout -b feat/my-feature develop`
2. Push and open a PR to `develop`. CI runs automatically.
3. Merge to `develop` when CI passes.
4. When `develop` is ready for release, prepare a release PR from `develop` → `master` via `make release-prepare`.
5. After merging to `master` and confirming CI passes on `master`, tag and push via `make release-publish`.

## CI / Release

- **CI** (`.github/workflows/ci.yml`): Runs on push/PR to `master` and `develop`. Checks fmt, clippy, Rust tests on Linux/macOS/Windows, and macOS menu-bar Swift tests.
- **Release** (`.github/workflows/release.yml`): Triggers on `v*` tags. Runs a `verify` gate (tag/version check, fmt, clippy, tests, `cargo publish --dry-run`) before building binaries for 5 targets, creating a GitHub release, publishing to crates.io, and updating Homebrew.

### Semi-Automatic Release Commands (`scripts/release.sh`)

```bash
make release-check               # run full local verification + cargo package/publish --dry-run
make release-rc VERSION=0.2.8-alpha.1
                                 # pre-release from develop (GitHub Release only; NO crates.io publish)
make release-prepare BUMP=patch  # from develop: create release/vX.Y.Z, bump Cargo.toml/Cargo.lock, verify, prompt push/PR
make release-publish             # from master: verify CI on HEAD, dry-run publish, prompt confirmation, push vX.Y.Z tag
```

## Code Conventions

- **New JSONL sources**: Implement `JsonlSourceConfig` (~15 lines) and use `JsonlSource<C>` from `source/jsonl_source.rs`
- **Cline-derived sources**: Implement `ClineSourceConfig` and use `ClineDerivedSource<C>` from `source/cline_format.rs`
- **SQLite sources**: See `source/opencode.rs` for the pattern — open read-only, busy_timeout, `json_extract` for JSON columns
- **Timestamps**: Always use `timestamp::parse_timestamp()`, never inline parsing
- **File discovery**: Each `Source` implements `discover_files()` using helpers from `source/discover.rs` (`collect_by_ext`, `walk_by_ext`). No glob crate — use bounded `read_dir` walking only.
- **Display names**: Use `display.rs` functions (`display_client`, `display_model`, `infer_api_provider`) for presentation
- **API provider detection**: Prefix model names with `vertexai.`, `openai/`, `bedrock/`, etc. so `infer_api_provider` works. `cost.rs::find_pricing` strips `vertexai.` before lookup.
- **Errors**: Skip malformed lines with `continue`, warnings to stderr only
- **Pure functions**: Annotate with `#[must_use]`
- **Pre-filtering**: JSONL parsers should `line.contains()` check before `serde_json::from_str` to skip non-matching lines cheaply
- **BufReader**: Use `BufReader::with_capacity(64 * 1024, file)` for line-by-line parsers

## Content Policy

- **Never reference other tools by name** in README, comments, commit messages, or documentation. No comparisons, no "inspired by X", no "like Y". tokemon stands on its own.
- File paths that contain third-party tool names (e.g., `~/.config/tokscale/cursor-cache/`) are acceptable since those are factual filesystem locations.
