.DEFAULT_GOAL := build
.PHONY: build release test fmt lint clean

build: ## Build mindweaver-core
	cargo build -p mindweaver-core

# The optimized build, which CI's `build (core)` runs once the debug build
# has passed, so a crate that compiles only in debug fails before a release
# is cut from it. `--locked`, as the CLI's release is: a lockfile the build
# would change fails rather than is rewritten.
release: ## Release (optimized) build of mindweaver-core
	cargo build -p mindweaver-core --release --locked

test: ## Run mindweaver-core tests
	cargo test -p mindweaver-core $(CARGO_TEST_FLAGS)

fmt: ## Format mindweaver-core
	cargo fmt -p mindweaver-core

lint: ## Clippy mindweaver-core
	cargo clippy -p mindweaver-core --all-targets

clean: ## Remove mindweaver-core build artifacts
	cargo clean -p mindweaver-core
