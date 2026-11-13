.DEFAULT_GOAL := build
.PHONY: build release release/check test run fmt lint clean

# Without the panel first, so a build that leaves out the default-on `ui`
# feature is seen to build, and the binaries left in target/ are the default
# build's: cargo puts those back even when nothing of them needs building.
build: ## Build mindweaver-cli (binaries: mind, mindweaver), first without the panel
	cargo build -p mindweaver-cli --no-default-features
	cargo build -p mindweaver-cli

# The panel first, which the server embeds as it compiles: a release built
# without it would ship a `ui` that serves only a page saying so. Built
# where Node and pnpm run, as nothing else here needs them, and otherwise
# taken from apps/web/dist as it is, with a warning (`make
# web/build/if-node`), which is how CI's `build (cli)` embeds the panel
# `test (web)` built. PNPM reaches the panel's Makefile.
release: ## Release build for a Rust target triple (TARGET=aarch64-apple-darwin), the panel built first where Node is
ifndef TARGET
	$(error TARGET is required, e.g. make release TARGET=aarch64-apple-darwin)
endif
	$(MAKE) -C ../web build/if-node
	cargo build -p mindweaver-cli --release --locked --target $(TARGET)

# That the release for TARGET serves the panel from the binary, once
# apps/web/dist is gone, which the script refuses to run without. A script
# rather than a test: only a release with the panel embedded can pass it,
# and `cargo test` builds neither. CI runs it on the release it builds.
release/check: ## Check the release for TARGET serves the panel from the binary (remove apps/web/dist first)
ifndef TARGET
	$(error TARGET is required, e.g. make release/check TARGET=x86_64-unknown-linux-gnu)
endif
	../../scripts/serves-panel.sh ../../target/$(TARGET)/release/mindweaver

# Then the unit tests without the panel, among them the one only that build
# compiles, that `ui` is no command there. Not the suites: compiling each
# again would cost far more than what it could find. All of them rather
# than that one by name, which a rename would leave matching nothing.
test: ## Run mindweaver-cli tests, then its unit tests without the panel
	cargo test -p mindweaver-cli $(CARGO_TEST_FLAGS)
	cargo test -p mindweaver-cli --no-default-features --lib $(CARGO_TEST_FLAGS)

run: ## Run the CLI (pass args via ARGS="list")
	cargo run -p mindweaver-cli -- $(ARGS)

fmt: ## Format mindweaver-cli
	cargo fmt -p mindweaver-cli

lint: ## Clippy mindweaver-cli
	cargo clippy -p mindweaver-cli --all-targets

clean: ## Remove mindweaver-cli build artifacts
	cargo clean -p mindweaver-cli
