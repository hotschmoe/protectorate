VERSION ?= $(shell git describe --tags --match 'v[0-9]*' --always --dirty 2>/dev/null || echo 0.0.0-dev)
GIT_COMMIT ?= $(shell git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)
BUILD_NAME ?= $(or $(shell printf '%s' '$(VERSION)' | sed -nE 's/^v?([0-9]+\.[0-9]+\.[0-9]+).*/\1/p'),0.0.0)
BUILD_NUMBER ?= $(shell git rev-list --count HEAD 2>/dev/null || echo 0)
APP_VERSION_FLAGS := --build-name=$(BUILD_NAME) --build-number=$(BUILD_NUMBER) --dart-define=PROTECTORATE_VERSION=$(VERSION)

-include .env.e2e
export PT_E2E_PI_PROVIDER PT_E2E_PI_MODEL ZIG \
	PT_RELEASE_WIN_HOST PT_RELEASE_WIN_DIR PT_RELEASE_WIN_FLUTTER PT_RELEASE_REPO \
	PT_RELEASE_APK_HOST PT_RELEASE_APK_SSH_KEY PT_RELEASE_APK_PATH PT_RELEASE_APK_ANDROID_HOME \
	PT_RELEASE_APK_SIGNING PT_RELEASE_APKSIGNER PT_RELEASE_ARM64_NODE PT_RELEASE_ARM64_DIR \
	PT_RELEASE_ARM64_PATH PT_RELEASE_ARM64_TIMEOUT PT_RELEASE_HUB_CLI PT_RELEASE_SERVE_ADDR \
	PT_RELEASE_WORK_DIR

ZIG ?= zig
FLUTTER ?= flutter
DART ?= dart
WEB_DIR ?= app/build/web
ZIG_FLAGS := -Doptimize=safe -Dversion=$(VERSION) -Dgit-commit=$(GIT_COMMIT) -Dweb-dir=$(WEB_DIR)

.PHONY: all build release-all check-levels gen check-gen dst web app-check e2e check version \
	release-assets release-snapshot release-publish

version:
	@echo '$(VERSION)'

all: build

build:
	$(ZIG) build $(ZIG_FLAGS)

release-all:
	$(ZIG) build release-all $(ZIG_FLAGS)

check-levels:
	$(ZIG) build check-levels

gen:
	$(ZIG) build gen

check-gen:
	$(ZIG) build check-gen

dst:
	$(ZIG) build -Doptimize=safe dst -- --seeds 300 --steps 1500

web:
	cd app && $(FLUTTER) build web --release --no-web-resources-cdn $(APP_VERSION_FLAGS)

app-check:
	cd app && $(FLUTTER) analyze

e2e:
	bash scripts/zig-e2e.sh

release-assets:
	VERSION=$(VERSION) APP_VERSION_FLAGS="$(APP_VERSION_FLAGS)" scripts/release.sh assets

release-snapshot:
	VERSION=$(VERSION) APP_VERSION_FLAGS="$(APP_VERSION_FLAGS)" scripts/release.sh assets --snapshot

release-publish:
	VERSION=$(VERSION) APP_VERSION_FLAGS="$(APP_VERSION_FLAGS)" scripts/release.sh publish

check: check-levels check-gen app-check build dst release-all e2e
	@echo "check: all steps passed"
