# The git tag is the one source of truth for the version. Every build takes
# VERSION from here; app/pubspec.yaml carries a placeholder that is always
# overridden (see check-version).
#
#   VERSION       git describe: v1.2.3, v1.2.3-4-gabc1234, any of them with
#                 -dirty, or a bare hash when there is no tag. Rust binaries
#                 (PROTECTORATE_BUILD_VERSION) and the app's
#                 PROTECTORATE_VERSION (shown in the UI and compared with the
#                 hub's) carry it verbatim.
#   BUILD_NAME    the app's --build-name: the x.y.z core of the nearest tag
#                 (0.0.0 without one). Commits past the tag and -dirty do not
#                 show here, only in VERSION.
#   BUILD_NUMBER  the app's --build-number (Android versionCode): commits on
#                 HEAD, which only grows, so every build sorts after the last.
#                 CI needs a full clone (fetch-depth: 0).
VERSION ?= $(shell git describe --tags --match 'v[0-9]*' --always --dirty 2>/dev/null || echo dev)
BUILD_NAME ?= $(or $(shell printf '%s' '$(VERSION)' | sed -nE 's/^v?([0-9]+\.[0-9]+\.[0-9]+).*/\1/p'),0.0.0)
BUILD_NUMBER ?= $(shell git rev-list --count HEAD 2>/dev/null || echo 0)
APP_VERSION_FLAGS := --build-name=$(BUILD_NAME) --build-number=$(BUILD_NUMBER) --dart-define=PROTECTORATE_VERSION=$(VERSION)
# APP_FLAGS adds flutter build flags.
APP_FLAGS ?=

.PHONY: gen check-gen build test check app-check app-build-web apk app-build-linux app-build-windows version version-app check-version

MCP_CATALOG := rust/crates/protectorate-mcpbridge/tests/testdata/mcp_tools.json
DESCRIPTOR := rust/crates/protectorate-wire/assets/protectorate.binpb
RUST_API := rust/crates/protectorate-wire/src/v1.rs

gen:
	buf lint
	buf generate
	sh rust/crates/protectorate-wire/scripts/update-descriptor.sh
	cargo run --quiet --locked -p protectorate-protogen -- $(DESCRIPTOR) protectorate.v1 $(RUST_API)
	cargo run --quiet --locked -p protectorate-mcpbridge -- --allow '*' --list > $(MCP_CATALOG)

check-gen:
	buf lint
	buf generate
	sh rust/crates/protectorate-wire/scripts/update-descriptor.sh --check
	cargo run --quiet --locked -p protectorate-protogen -- $(DESCRIPTOR) protectorate.v1 $(RUST_API)
	cargo run --quiet --locked -p protectorate-mcpbridge -- --allow '*' --list > $(MCP_CATALOG)
	@git diff --exit-code --stat -- app/lib/gen $(MCP_CATALOG) $(RUST_API) || (echo "generated code is stale: commit the output of make gen"; exit 1)
	@test -z "$$(git status --porcelain -- app/lib/gen)" || (echo "untracked generated files: commit the output of make gen"; exit 1)

build:
	PROTECTORATE_BUILD_VERSION='$(VERSION)' cargo build --locked --release --workspace --bins

test:
	cargo test --locked --workspace

check:
	cargo fmt --all -- --check
	cargo clippy --locked --workspace --all-targets -- -D warnings
	$(MAKE) test app-check check-version

app-check:
	cd app && flutter analyze

app-build-web:
	cd app && dart run tool/build_web.dart $(APP_VERSION_FLAGS) $(APP_FLAGS)

apk:
	cd app && flutter build apk $(APP_VERSION_FLAGS) $(APP_FLAGS)

app-build-linux:
	cd app && flutter build linux $(APP_VERSION_FLAGS) $(APP_FLAGS)

app-build-windows:
	cd app && flutter build windows $(APP_VERSION_FLAGS) $(APP_FLAGS)

version:
	@echo $(VERSION)

version-app:
	@echo $(BUILD_NAME)+$(BUILD_NUMBER)

check-version:
	@sh scripts/check-version.sh
