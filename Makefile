VERSION ?= $(shell git describe --tags --match 'v[0-9]*' --always --dirty 2>/dev/null || echo dev)

.PHONY: build release release-assets release-publish web test test-paid check check-deps check-gen gen fmt app-check

DART ?= dart
FLUTTER ?= flutter
# The release binary embeds this Flutter web build when it exists (make web first).
WEB_DIR ?= $(CURDIR)/app/build/web
API_DART := app/lib/api/api.g.dart
# Inside app/ so dart format sees the app's language version and settings; .dart_tool is ignored.
API_TMP := app/.dart_tool/apigen/api.g.dart

build:
	PROTECTORATE_BUILD_VERSION='$(VERSION)' cargo build --locked --workspace

release:
	PROTECTORATE_BUILD_VERSION='$(VERSION)' PROTECTORATE_WEB_DIR='$(WEB_DIR)' cargo build --locked --release --workspace --bins

# Same-origin web app: the hub serves it, so no CDN (canvaskit and fonts are local).
web:
	cd app && $(FLUTTER) build web --release --no-web-resources-cdn

app-check:
	cd app && $(FLUTTER) analyze

# Every release asset into dist/<version>/ with SHA256SUMS, built here and on officewin (SPEC 6).
release-assets:
	bash scripts/release.sh assets

# gh release create with dist/<version>/; a dry run unless PUBLISH=1.
release-publish:
	bash scripts/release.sh publish

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
	$(MAKE) test check-deps check-gen app-check

check-deps:
	sh scripts/check-deps.sh

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
