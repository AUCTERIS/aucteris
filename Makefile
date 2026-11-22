# Aucteris dev tasks — thin wrapper over cargo so `make <target>` matches CI.
# CI (.github/workflows/ci.yml) is the source of truth; keep targets in sync.

.PHONY: setup fmt lint test doc deny check run-cli run-desktop clean

setup: ## Install pinned toolchain components + cargo tools used by CI
	rustup component add rustfmt clippy llvm-tools-preview
	cargo install cargo-nextest --locked
	cargo install cargo-deny --locked
	cargo install cargo-machete --locked
	cargo install cargo-llvm-cov --locked

fmt: ## cargo fmt --check (CI gate)
	cargo fmt --all -- --check

fmt-fix: ## Apply formatting
	cargo fmt --all

lint: ## clippy -D warnings (CI gate)
	cargo clippy --workspace --all-targets --locked -- -D warnings

machete: ## unused-dependency check (CI hygiene job)
	cargo machete

deny: ## supply-chain gate (CI deny job)
	cargo deny check

test: ## nextest + doctests (CI rust job)
	cargo nextest run --workspace --locked
	cargo test --workspace --doc --locked

coverage: ## llvm-cov lcov report (same as coverage.yml)
	cargo llvm-cov --workspace --lcov --output-path lcov.info --locked

check: fmt lint machete deny test ## Everything CI runs on a PR

doc: ## build rustdoc, warn on broken intra-doc links
	RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps

run-cli: ## aucteris CLI
	cargo run -p aucteris-cli --

run-desktop: ## GPUI desktop app
	cargo run -p aucteris-desktop

clean: ## drop build artifacts
	cargo clean
