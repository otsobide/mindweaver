.DEFAULT_GOAL := build
.PHONY: build release test fmt lint clean

build: ## Build mindweaver-server
	cargo build -p mindweaver-server

# The optimized build, which CI's `build (server)` runs once the debug build
# has passed, as core's is. It embeds whatever apps/web/dist holds, nothing
# in that job, which `allow_missing` lets it build without: the release that
# has to carry the panel is the CLI's, which `make cli/release/check` holds
# to serving it.
release: ## Release (optimized) build of mindweaver-server
	cargo build -p mindweaver-server --release --locked

test: ## Run mindweaver-server tests
	cargo test -p mindweaver-server $(CARGO_TEST_FLAGS)

fmt: ## Format mindweaver-server
	cargo fmt -p mindweaver-server

lint: ## Clippy mindweaver-server
	cargo clippy -p mindweaver-server --all-targets

clean: ## Remove mindweaver-server build artifacts
	cargo clean -p mindweaver-server
