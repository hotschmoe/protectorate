VERSION ?= $(shell git describe --tags --match 'v[0-9]*' --always --dirty 2>/dev/null || echo dev)

.PHONY: build release test check check-deps fmt

build:
	PROTECTORATE_BUILD_VERSION='$(VERSION)' cargo build --locked --workspace

release:
	PROTECTORATE_BUILD_VERSION='$(VERSION)' cargo build --locked --release --workspace --bins

test:
	cargo test --locked --workspace

fmt:
	cargo fmt --all

check:
	cargo fmt --all -- --check
	cargo clippy --locked --workspace --all-targets -- -D warnings
	$(MAKE) test check-deps

check-deps:
	sh scripts/check-deps.sh
