# The git tag is the one source of truth for the version (vX.Y.Z, or vX.Y.Z-PRE such as
# v0.7.0-dev.1 for the dev channel). Every build takes VERSION from here; Cargo.toml's workspace
# version and app/pubspec.yaml carry placeholders that are never used (make check-version).
#
#   VERSION       git describe: v1.2.3 on a tag, v1.2.3-4-gabc1234 past one, either with
#                 -dirty, or a bare hash with no tag. The Rust binaries
#                 (PROTECTORATE_BUILD_VERSION) and the app (PROTECTORATE_VERSION) carry it
#                 verbatim; scripts/release.sh builds a release only from a clean tag.
#   BUILD_NAME    the app's --build-name: the x.y.z core of VERSION (0.0.0 without a tag).
#   BUILD_NUMBER  the app's --build-number (Android versionCode): commits on HEAD, which only
#                 grows.
VERSION ?= $(shell git describe --tags --match 'v[0-9]*' --always --dirty 2>/dev/null || echo dev)
BUILD_NAME ?= $(or $(shell printf '%s' '$(VERSION)' | sed -nE 's/^v([0-9]+\.[0-9]+\.[0-9]+).*/\1/p'),0.0.0)
BUILD_NUMBER ?= $(shell git rev-list --count HEAD 2>/dev/null || echo 0)
APP_VERSION_FLAGS := --build-name=$(BUILD_NAME) --build-number=$(BUILD_NUMBER) --dart-define=PROTECTORATE_VERSION=$(VERSION)
CARGO_VERSION := PROTECTORATE_BUILD_VERSION='$(VERSION)'

# Machine-specific settings for tests and releases, never committed: KEY=value lines in the
# gitignored .env.e2e, read here when it exists (a value there beats the environment; `make
# VAR=value` beats both). The tests skip, and the scripts say what to set, without them:
#   PT_E2E_PI_PROVIDER, PT_E2E_PI_MODEL   Pi provider and model of a local OpenAI-compatible
#                                         endpoint (the pi_local e2e tests, scripts/app-e2e.sh)
#   PT_RELEASE_WIN_HOST, PT_RELEASE_WIN_DIR, ZIG, PT_RELEASE_REPO, PT_RELEASE_APK_*,
#   PT_RELEASE_ARM64_*, PT_RELEASE_HUB_CLI, PT_RELEASE_SERVE_ADDR   scripts/release.sh
#   PT_NET_BIND, PT_NET_LINUX_HOST, PT_NET_LINUX_KEY, PT_NET_WIN_HOST   scripts/net-remote.sh
-include .env.e2e
export PT_E2E_PI_PROVIDER PT_E2E_PI_MODEL PT_RELEASE_WIN_HOST PT_RELEASE_WIN_DIR ZIG PT_RELEASE_REPO

.PHONY: build release release-assets release-snapshot release-publish web test test-heavy test-paid check fmt-check clippy check-deps check-levels check-gen check-version check-fuzz gen fmt app-check version

DART ?= dart
FLUTTER ?= flutter
# The release binary embeds this Flutter web build when it exists (make web first).
WEB_DIR ?= $(CURDIR)/app/build/web
API_DART := app/lib/api/api.g.dart
# Inside app/ so dart format sees the app's language version and settings; .dart_tool is ignored.
API_TMP := app/.dart_tool/apigen/api.g.dart

version:
	@echo '$(VERSION)'

build:
	$(CARGO_VERSION) cargo build --locked --workspace

release:
	$(CARGO_VERSION) PROTECTORATE_WEB_DIR='$(WEB_DIR)' cargo build --locked --release --workspace --bins

# Same-origin web app: the hub serves it, so no CDN (canvaskit and fonts are local).
web:
	cd app && $(FLUTTER) build web --release --no-web-resources-cdn $(APP_VERSION_FLAGS)

app-check:
	cd app && $(FLUTTER) analyze

# Every release asset into dist/<version>/ with SHA256SUMS, built here and on a Windows host
# over ssh (SPEC 6). Only from a clean vX.Y.Z[-PRE] tag on HEAD; release-snapshot builds any
# VERSION, for trying the build.
release-assets:
	VERSION='$(VERSION)' APP_VERSION_FLAGS='$(APP_VERSION_FLAGS)' bash scripts/release.sh assets

release-snapshot:
	VERSION='$(VERSION)' APP_VERSION_FLAGS='$(APP_VERSION_FLAGS)' bash scripts/release.sh assets --snapshot

# gh release create with dist/<version>/; a dry run unless PUBLISH=1.
release-publish:
	VERSION='$(VERSION)' bash scripts/release.sh publish

# The workspace's e2e tests (their binaries side by side), then every committed fuzz seed
# through its real fuzz targets.
test:
	@failed=""; $(CARGO_VERSION) sh scripts/test.sh || failed="$$failed e2e"; \
	  $(CARGO_VERSION) sh scripts/replay-seeds.sh || failed="$$failed seeds"; \
	  if [ -n "$$failed" ]; then echo "test failed:$$failed"; exit 1; fi

test-heavy:
	$(CARGO_VERSION) cargo test --locked --workspace --test hub -- --ignored

# Also runs the Claude, Codex and Grok e2e tests, which spend a few small turns of quota.
test-paid:
	$(CARGO_VERSION) PT_E2E_PAID=1 cargo test --locked --workspace

fmt:
	cargo fmt --all

check:
	@failed=""; \
	  for step in fmt-check clippy check-fuzz test check-deps check-levels check-gen app-check check-version; do \
	    $(MAKE) $$step || failed="$$failed $$step"; \
	  done; \
	  if [ -n "$$failed" ]; then echo "check failed:$$failed"; exit 1; fi; \
	  echo "check: all steps passed"

fmt-check:
	cargo fmt --all -- --check

clippy:
	$(CARGO_VERSION) cargo clippy --locked --workspace --all-targets -- -D warnings

check-fuzz:
	@failed=""; for dir in crates/*/fuzz; do \
	  $(CARGO_VERSION) CARGO_TARGET_DIR='$(if $(CARGO_TARGET_DIR),$(CARGO_TARGET_DIR),$(CURDIR)/target)/fuzz-seeds' cargo build --locked --manifest-path "$$dir/Cargo.toml" --bins || failed="$$failed $$dir"; \
	  done; if [ -n "$$failed" ]; then echo "fuzz build failed:$$failed"; exit 1; fi

check-deps:
	sh scripts/check-deps.sh

# Fails when a crate's modules use each other in a cycle or use an item of their crate root.
check-levels:
	cargo run --locked --quiet --bin pt-levels -- crates

# Fails when a build path could take its version from anywhere but VERSION above.
check-version:
	sh scripts/check-version.sh

# Writes the Dart client from crates/pt-api (pt-apigen, then dart format).
gen:
	mkdir -p $(dir $(API_TMP))
	cargo run --locked --quiet --bin pt-apigen -- $(API_TMP)
	$(DART) format $(API_TMP) >/dev/null
	cp $(API_TMP) $(API_DART)

# Fails when the committed Dart client differs from what crates/pt-api generates.
check-gen:
	mkdir -p $(dir $(API_TMP))
	cargo run --locked --quiet --bin pt-apigen -- $(API_TMP)
	$(DART) format $(API_TMP) >/dev/null
	@cmp -s $(API_TMP) $(API_DART) || { echo "check-gen: $(API_DART) is stale; run make gen"; exit 1; }
	@echo "check-gen: ok"
