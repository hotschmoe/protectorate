# Protectorate (Zig 0.17.1 Rewrite)

Protectorate is an orchestrator for long-running AI coding agent sleeves (`claude`, `codex`, `grok`, `antigravity`/`agy`, `pi`) and remote fleet nodes (`protectorate-node`), written in Zig 0.17.1 using TigerStyle invariants, Deterministic Simulation Testing (VOPR), zero external Zig dependencies (`std` only), and an embedded Flutter web/mobile UI.

## Module Architecture & Strict Tiering

Every source file under `src/` belongs to one of six monotonic dependency layers (`L0` -> `L5`), enforced at build time by `zig build check-levels` (`src/tools/levels.zig`):

- **L0 (`tiger.zig`, `version.zig`)**: Fail-fast TigerStyle assertions and build metadata. Zero local imports.
- **L1 (`qr.zig`, `json_util.zig`, `net/{frame,noise}.zig`, `journal/codec.zig`, `history/token.zig`, `core/model.zig`, `quota/select.zig`, `update/version.zig`)**: Pure deterministic domain primitives and codecs. Imports L0 only.
- **L2 (`api/schema.zig`, `net/msg.zig`, `core/*` including `core/admission.zig`, `drivers/{protocol,claude,codex,grok,pi}.zig`, `history/{extract,segment}.zig`, `sandbox/spec.zig`)**: State machines (hub reducer + node admission authority), binary formats, protocol adapters, BM25 ranking, sandbox arg builders, and single-source API schema. Imports L0-L1 or acyclic same-subsystem files.
- **L3 (`journal/store.zig`, `net/transport.zig`, `history/index.zig`, `quota/readers.zig`, `sandbox/exec.zig`, `drivers/session.zig`, `update/store.zig`, `doctor/doctor.zig`)**: Effectful OS/IO components (files, processes, HTTP/TCP/Noise sockets). Strict rule: imports L0-L2 ONLY (never another L3 module).
- **L4 (`node/*`, `hub/*`, `cli/*`)**: Daemons, HTTP/WS server, MCP Envoy server, fleet multiplexer, CLI client. May import L0-L3 and acyclic same-subsystem files.
- **L5 (`main_*.zig`, `dst/*`, `tools/*`)**: Executable entry points, VOPR deterministic simulator, codegen (`pt-apigen`), web packer (`pt-embed-web`), and level checker (`pt-levels`).

No `test "..."` unit test blocks exist anywhere under `src/` (`pt-levels` rejects them). Reliability is verified through `vopr` multi-hundred-seed DST simulation (`zig build dst`) and real-binary E2E suites (`scripts/zig-e2e.sh`).

## Key Build & Run Commands

```sh
zig build check-levels          # Verify L0-L5 DAG and zero unit-test blocks
zig build gen                   # Regenerate app/lib/api/api.g.dart from src/api/schema.zig
zig build check-gen             # Fail if generated Dart API drifted from schema
zig build -Doptimize=safe       # Compile protectorate + protectorate-node
zig build release-all           # Cross-compile linux-x86_64/aarch64, windows-gnu, macos-aarch64
zig build -Doptimize=safe dst -- --seeds 300 --steps 1500
```
