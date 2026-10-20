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
# Keep tsnet transport, DNS, NAT traversal, Android integration and system roots.
# See docs/footprint.md for feature measurements and integration checks.
TAILSCALE_TAGS := \
	ts_omit_advertiseexitnode ts_omit_advertiseroutes ts_omit_appconnectors \
	ts_omit_aws ts_omit_bird ts_omit_c2n ts_omit_cachenetmap \
	ts_omit_captiveportal ts_omit_capture ts_omit_cliconndiag ts_omit_clientmetrics \
	ts_omit_clientupdate ts_omit_cloud ts_omit_colorable ts_omit_completion \
	ts_omit_completion_scripts ts_omit_conn25 ts_omit_dbus ts_omit_debug \
	ts_omit_debugeventbus ts_omit_desktop_sessions ts_omit_doctor ts_omit_drive \
	ts_omit_favorites ts_omit_flashappliance ts_omit_hujsonconf ts_omit_identityfederation \
	ts_omit_iptables ts_omit_kube ts_omit_linkspeed ts_omit_linuxdnsfight \
	ts_omit_logtail ts_omit_netlog ts_omit_networkmanager ts_omit_oauthkey \
	ts_omit_osrouter ts_omit_outboundproxy ts_omit_peerapiclient ts_omit_peerapiserver \
	ts_omit_portlist ts_omit_posture ts_omit_qrcodes ts_omit_relayserver \
	ts_omit_remoteconfig ts_omit_resolved ts_omit_runtimemetrics ts_omit_sdnotify \
	ts_omit_serviceclientprefs ts_omit_ssh ts_omit_synology \
	ts_omit_syslog ts_omit_syspolicy ts_omit_systray ts_omit_taildrop \
	ts_omit_tailnetlock ts_omit_tap ts_omit_tpm ts_omit_tundevstats \
	ts_omit_unixsocketidentity ts_omit_useexitnode ts_omit_usermetrics ts_omit_useroutes \
	ts_omit_wakeonlan ts_omit_webbrowser ts_omit_webclient
TARGETS := linux/amd64 linux/arm64 windows/amd64 windows/arm64 android/arm64

.PHONY: gen check-gen build build-tags test vet cross app-check app-build-web apk app-build-linux version version-app check-version check

gen:
	buf lint
	buf generate
	buf build -o internal/mcpbridge/image.binpb

check-gen: gen
	@git diff --exit-code --stat -- gen app/lib/gen internal/mcpbridge/image.binpb || (echo "generated code is stale: commit the output of make gen"; exit 1)
	@test -z "$$(git status --porcelain -- gen app/lib/gen)" || (echo "untracked generated files: commit the output of make gen"; exit 1)

build:
	CGO_ENABLED=0 go build -trimpath -tags '$(TAILSCALE_TAGS)' -ldflags '$(LDFLAGS)' -o bin/protectorate ./cmd/protectorate

test:
	go vet -tags '$(TAILSCALE_TAGS)' ./...
	go test -tags '$(TAILSCALE_TAGS)' -race ./...

build-tags:
	@echo '$(TAILSCALE_TAGS)'

cross:
	@for t in $(TARGETS); do \
		os=$${t%/*}; arch=$${t#*/}; ext=; [ $$os = windows ] && ext=.exe; \
		out=bin/protectorate-$$os-$$arch$$ext; \
		CGO_ENABLED=0 GOOS=$$os GOARCH=$$arch go build -trimpath -tags '$(TAILSCALE_TAGS)' -ldflags '$(LDFLAGS)' -o $$out ./cmd/protectorate || exit 1; \
		ls -l $$out; \
	done

app-check:
	cd app && flutter analyze && flutter test

app-build-web:
	cd app && flutter build web $(APP_VERSION_FLAGS) $(APP_FLAGS)

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
