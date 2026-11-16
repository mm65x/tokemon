.PHONY: build test macos-test clean install dev-install help lint fmt fmt-check smoke ci check dev-top sandbox-top setup-hooks release-check release-rc release-prepare release-publish

BINARY  := target/release/tokemon
UNAME_S := $(shell uname -s)

help: ## Show this help
	@grep -E '^[a-zA-Z_-]+:.*?## .*$$' $(MAKEFILE_LIST) | \
		awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-16s\033[0m %s\n", $$1, $$2}'

build: ## Build release binary
	cargo build --release

test: ## Run all Rust unit & integration tests
	cargo test

macos-test: ## Run macOS menu-bar Swift tests (Darwin only)
ifeq ($(UNAME_S),Darwin)
	swift test --package-path macos
else
	@echo "Skipping macos-test on $(UNAME_S)"
endif

install: build ## Install release binary to ~/.cargo/bin
	mkdir -p $(HOME)/.cargo/bin
	install -m 755 $(BINARY) $(HOME)/.cargo/bin/tokemon
ifeq ($(UNAME_S),Darwin)
	codesign -s - --force $(HOME)/.cargo/bin/tokemon
endif
	$(HOME)/.cargo/bin/tokemon --version

dev-install: install ## Build release binary and install locally

clean: ## Remove build artifacts
	cargo clean

lint: ## Run pedantic clippy
	cargo clippy -- -W clippy::pedantic -A clippy::module_name_repetitions -D warnings

fmt: ## Format code
	cargo fmt

fmt-check: ## Check formatting
	cargo fmt -- --check

smoke: ## Run isolated CLI and MCP smoke tests
	./scripts/smoke-test.sh

ci: fmt-check lint test macos-test smoke ## Run all local CI checks

check: ci ## Alias for full local verification

dev-top: ## Launch tokemon top against real logs with scratch cache/config
	./scripts/sandbox.sh live-top

sandbox-top: ## Launch tokemon top --offline in a seeded 120-day sandbox
	./scripts/sandbox.sh top

setup-hooks: ## Enable repo git hooks and ensure scripts are executable
	git config core.hooksPath .githooks
	chmod +x .githooks/* scripts/*.sh

release-check: ## Run release verification including cargo publish --dry-run
	./scripts/release.sh check

release-rc: ## Tag and push a pre-release from develop (VERSION=0.2.8-alpha.1)
	./scripts/release.sh rc $(VERSION)

release-prepare: ## Bump version on release/vX.Y.Z from develop (BUMP=patch|minor|major|X.Y.Z)
	./scripts/release.sh prepare $(or $(BUMP),$(VERSION),patch)

release-publish: ## Verify master CI, confirm, and push vX.Y.Z release tag
	./scripts/release.sh publish
