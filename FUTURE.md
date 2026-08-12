# Tokemon - Future Features Roadmap

Features deferred from the PoC/MVP, organized by priority.

## High Priority

### HTTP Proxy Mode
Intercept LLM API calls in real-time by setting `BASE_URL` to a local proxy . Captures usage data at the wire level, independent of log file formats.
- Implementation: `hyper` reverse proxy + `tokio` async runtime
- Config: `tokemon proxy --port 8080 --upstream https://api.anthropic.com`

### Provider API Polling
Query provider billing/usage APIs directly for accurate cost data:
- Claude: `api.anthropic.com/v1/organizations/usage`
- OpenAI/Codex: `api.openai.com/v1/usage`
- OpenRouter: `openrouter.ai/api/v1/credits`
- Google AI: Vertex AI billing API

### Real-time File Watching
Use `notify` crate to watch log directories for changes. Incremental O(1) updates instead of full re-parse. Enables live dashboard.

### Data Preservation Cache
Cache parsed data to survive provider log rotation (e.g., Claude Code's 30-day deletion policy). SQLite cache at `~/.cache/tokemon/usage.db`.

## Medium Priority

### Interactive TUI Dashboard
`ratatui`-based terminal UI with:
- Keyboard navigation (vim keys)
- Sparkline charts for usage trends
- Tab-based views (daily/weekly/monthly/by-provider)
- Live-updating stats

### macOS Menu Bar App
Phase 1 local alpha is implemented in `macos/` with:
- Quick usage summary in the menu dropdown
- Tokens, cost, and combined headline metrics
- Today, week, month, and all-time scopes using UTC date boundaries
- Configurable executable path, refresh interval, metric, and scope
- Explicit empty, error, timeout, and unavailable-cost states

Remaining work is packaging, signing, distribution, provider filtering, and
budget notifications. The alpha intentionally invokes the local status command
directly and does not add a daemon, IPC layer, telemetry, or network service.

### MCP Server Integration
Expose usage data as an MCP tool so AI assistants can self-monitor their token consumption. Useful for cost-aware agents.

## Lower Priority

### VS Code Extension
Webview dashboard showing:
- Session cost in status bar
- Per-file token attribution
- Usage charts

### System Tray Icon (Cross-platform)
- Windows/Linux equivalent of menu bar app
- Settings UI for provider configuration
- Budget notifications

### Cloud Dashboard / Cross-machine Sync
Web dashboard aggregating usage across machines. Optional sync via cloud storage or simple server.

### CSV/PDF Export
Export reports for expense reporting and team billing.

### Per-session Breakdown View
`tokemon sessions` command showing individual sessions with:
- Duration, tokens, cost per session
- Session descriptions (from first user message)

### Piebald SQLite Support
Reverse-engineer Piebald's `app.db` schema to parse usage data (rusqlite is already a dependency).

### Copilot Token Estimation
Use `tiktoken` to estimate token counts from Copilot chat sessions (which don't include token metadata).
