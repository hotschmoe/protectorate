# The git tag is the one source of truth for the version. Every build takes
# VERSION from here; app/pubspec.yaml carries a placeholder that is always
# overridden (see check-version).
#
#   VERSION       git describe: v1.2.3, v1.2.3-4-gabc1234, any of them with
#                 -dirty, or a bare hash when there is no tag. Go binaries and
#                 the app's PROTECTORATE_VERSION (shown in the UI and compared
#                 with the hub's) carry it verbatim.
#   BUILD_NAME    the app's --build-name: the x.y.z core of the nearest tag
#                 (0.0.0 without one). Commits past the tag and -dirty do not
#                 show here, only in VERSION.
#   BUILD_NUMBER  the app's --build-number (Android versionCode): commits on
#                 HEAD, which only grows, so every build sorts after the last.
#                 CI needs a full clone (fetch-depth: 0).
VERSION ?= $(shell git describe --tags --always --dirty 2>/dev/null || echo dev)
BUILD_NAME ?= $(or $(shell printf '%s' '$(VERSION)' | sed -nE 's/^v?([0-9]+\.[0-9]+\.[0-9]+).*/\1/p'),0.0.0)
BUILD_NUMBER ?= $(shell git rev-list --count HEAD 2>/dev/null || echo 0)
APP_VERSION_FLAGS := --build-name=$(BUILD_NAME) --build-number=$(BUILD_NUMBER) --dart-define=PROTECTORATE_VERSION=$(VERSION)
# APP_FLAGS adds flutter build flags, e.g. --dart-define=E2E_AUTOCONFIRM=true.
APP_FLAGS ?=
LDFLAGS := -s -w -X main.version=$(VERSION)
TARGETS := linux/amd64 linux/arm64 windows/amd64 windows/arm64 android/arm64

.PHONY: gen check-gen build test vet cross app-check app-build-web apk app-build-linux version version-app check-version check

gen:
	buf lint
	buf generate
	buf build -o internal/mcpbridge/image.binpb

check-gen: gen
	@git diff --exit-code --stat -- gen app/lib/gen internal/mcpbridge/image.binpb || (echo "generated code is stale: commit the output of make gen"; exit 1)
	@test -z "$$(git status --porcelain -- gen app/lib/gen)" || (echo "untracked generated files: commit the output of make gen"; exit 1)

build:
	CGO_ENABLED=0 go build -trimpath -ldflags '$(LDFLAGS)' -o bin/protectorate ./cmd/protectorate

test:
	go vet ./...
	go test -race ./...

cross:
	@for t in $(TARGETS); do \
		os=$${t%/*}; arch=$${t#*/}; ext=; [ $$os = windows ] && ext=.exe; \
		out=bin/protectorate-$$os-$$arch$$ext; \
		CGO_ENABLED=0 GOOS=$$os GOARCH=$$arch go build -trimpath -ldflags '$(LDFLAGS)' -o $$out ./cmd/protectorate || exit 1; \
		ls -l $$out; \
	done

app-check:
	cd app && flutter analyze && flutter test $${FLUTTER_TEST_CONCURRENCY:+--concurrency=$$FLUTTER_TEST_CONCURRENCY}

app-build-web:
	cd app && dart run tool/build_web.dart $(APP_VERSION_FLAGS) $(APP_FLAGS)

apk:
	cd app && flutter build apk $(APP_VERSION_FLAGS) $(APP_FLAGS)

app-build-linux:
	cd app && flutter build linux $(APP_VERSION_FLAGS) $(APP_FLAGS)

version:
	@echo $(VERSION)

version-app:
	@echo $(BUILD_NAME)+$(BUILD_NUMBER)

check-version:
	@sh scripts/check-version.sh

check:
	@test -z "$$(gofmt -l cmd internal)" || (gofmt -l cmd internal; echo "gofmt: files above need formatting"; exit 1)
	$(MAKE) test check-gen cross app-check check-version
