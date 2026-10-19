# CLAUDE.md

Guidance for Claude Code in this repository. Read PROBLEMS.md and ARCHITECTURE.md first. `archive/` and `references/` are read-only reference material.

## RULE 1 - ABSOLUTE (DO NOT EVER VIOLATE THIS)

You may NOT delete any file or directory unless I explicitly give the exact command **in this session**.

- This includes files you just created (tests, tmp files, scripts, etc.).
- You do not get to decide that something is "safe" to remove.
- If you think something should be removed, stop and ask. You must receive clear written approval **before** any deletion command is even proposed.

### Irreversible git and filesystem actions

Forbidden unless I give the exact command and explicit approval in the same message: `git reset --hard`, `git clean -fd`, `rm -rf`, or any command that can delete or overwrite code/data.

1. If you are not 100% sure what a command will delete, do not propose or run it.
2. Prefer safe tools: `git status`, `git diff`, copying to backups.
3. After approval, restate the command verbatim and list what it affects.
4. Record the authorizing text, the command and the time. Without that audit trail, treat the operation as never having happened.

## Code discipline

- NO EMOJIS anywhere.
- Hands-off toward harnesses: never wrap, patch, proxy or re-prompt an AI CLI. Use only its own interfaces (TUI or official headless protocol) and its own config/auth.
- The orchestration layer must be correct, robust and fast; spend complexity there when correctness needs it, never for speculation. No abstractions without a second real implementation. The bar for a new file or dependency is still high.
- Minimal comments, only where they add what the code cannot say.
- No bulk-modifying scripts (codemods, giant sed/regex refactors). Make mechanical changes in small explicit edits.
- Full migrations only: no compat shims, `_legacy`/`_old` names, v2 clones or "remove later" code. Change behavior and migrate every caller in the same commit.
- Errors wrap with context (`fmt.Errorf("...: %w", err)`); pass `context.Context` for cancellation; tests are table-driven.

## Layout and commands

```
proto/protectorate/v1/   API schema (the only API definition)
gen/                     generated Go, committed
app/                     Flutter client (app/lib/gen is generated Dart, committed)
cmd/protectorate/        cobra CLI (thin)
internal/hub/            Connect service implementations
```

```
make gen     # buf lint + generate (needs buf, protoc-gen-go, protoc-gen-connect-go via go install; protoc-gen-dart and protoc-gen-connect-dart via dart pub global activate protoc_plugin / connectrpc, on PATH from ~/.pub-cache/bin)
make build   # bin/protectorate
make app-check  # flutter analyze + test in app/
make test    # go vet + go test -race
make check-gen  # fails if generated Go/Dart/descriptor image are stale or uncommitted
make cross   # linux/windows amd64+arm64 and android/arm64 into bin/
gofmt -l .   # must print nothing
```

Never hand-edit `gen/`; change the .proto and run `make gen`. Go lives in `~/.local/go/bin`.
