VERSION ?= $(shell git describe --tags --always --dirty 2>/dev/null || echo dev)
LDFLAGS := -s -w -X main.version=$(VERSION)
TARGETS := linux/amd64 linux/arm64 windows/amd64 windows/arm64 android/arm64

.PHONY: gen check-gen build test vet cross app-check

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
	cd app && flutter analyze && flutter test
