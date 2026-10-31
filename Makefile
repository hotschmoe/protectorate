VERSION ?= $(shell git describe --tags --match 'v[0-9]*' --always --dirty 2>/dev/null || echo dev)

.PHONY: build release test test-paid check check-deps fmt

build:
	PROTECTORATE_BUILD_VERSION='$(VERSION)' cargo build --locked --workspace

release:
	PROTECTORATE_BUILD_VERSION='$(VERSION)' cargo build --locked --release --workspace --bins

test:
	cargo test --locked --workspace

# Also runs the Claude, Codex and Grok e2e tests, which spend a few small turns of quota.
test-paid:
	PT_E2E_PAID=1 cargo test --locked --workspace

fmt:
	cargo fmt --all

check:
	cargo fmt --all -- --check
	cargo clippy --locked --workspace --all-targets -- -D warnings
	$(MAKE) test check-deps

check-deps:
	sh scripts/check-deps.sh
