# API Schema, Flutter Web UI & Envoy MCP (`src/api/`, `src/tools/apigen.zig`, `src/cli/`)

## Single-Source Schema & Dart Codegen

`src/api/schema.zig` defines every wire enum (31), union (3), record struct (100+), and all 49 RPC methods (`methods`). `pt-apigen` (`src/tools/apigen.zig`) uses Zig comptime reflection to emit `app/lib/api/api.g.dart` directly into the Flutter project, and `zig build check-gen` fails if generated client bindings drift from the schema.

## Embedded Web UI & HTTP/WS Server (`src/hub/http_server.zig`, `src/tools/embed_web.zig`)

At build time `pt-embed-web` pre-compresses `app/build/web` files with gzip (`std.compress.flate.Compress`, keeping raw bytes only when compression does not shrink the asset) into the pure-Zig module `web_assets` embedded directly inside `protectorate`. Release builds (`zig build release-all` or `-Dstrip=true`) strip debug symbols while preserving `ReleaseSafe` checks. At runtime the hub's HTTP loop (docs/srcmd/network.md, "HTTP and WebSocket loop") serves:
- `GET /health`: unauthenticated liveness check.
- `POST /api/<Method>`: bearer-authenticated RPC dispatch (`PairDevice` is public one-time claim; `.owner` and `.envoy_only` scopes checked per `schema.methods`).
- `GET /api/events`: RFC 6455 WebSocket stream (accepting `Authorization: Bearer` header or `?access_token=` query param for browsers), emitting `snapshot` + `live` + incremental `HubStreamMessage` updates from `StreamHub`.
- `GET /*`: embedded Flutter SPA assets (serving pre-gzipped assets directly with `Content-Encoding: gzip` when accepted, or transparently inflating via `std.compress.flate.Decompress` for uncompressed clients).

## CLI & Envoy MCP (`src/cli/`)

`cli/commands.zig` provides the operator CLI (`serve`, `status`, `doctor`, `pair`, `token`, `quota`, `sleeve list|attach|stop|rm|prompt|transcript`, `node list|join-token|add|approve|update`, `hub update`, `harness update`, `job list|get|cancel|logs`, `history search`, `rpc`). All query and status commands render aligned human-readable tables by default and emit machine JSON with `--json`. `sleeve attach <id>` invokes `TakeoverTerminal` to release the hub's managed driver and hands off directly to the interactive harness CLI in the user's terminal.

`hub/api_dispatch.zig` and `hub/runtime.zig` enforce D9 owner-approval gating on Envoy action calls (`SpawnSleeve` past `d9_spawn_limit`, `RemoveSleeve`, `GitPush`, `RunJob`), block `EnvoyWait` on turn settlement and extract the Envoy transcript reply, and orchestrate fleet rollouts via `UpdateNodes` and harness updates via `UpdateHarnesses`. `protectorate --envoy` / `mcp` (`src/cli/mcp.zig`) serves JSON-RPC 2.0 over stdio exposing Envoy tools (`spawn_sleeve`, `send_message`, `get_transcript`, `wait_sleeve`, `run_job`, `search_history`, etc.).

`EnvoySend` queues vendor mail when the caller supplies an agent name or uses the envoy-only credential. Envoy sees `[from agent:<name>]` and the resulting turn is subject to D9. An unnamed send from the owner or a paired device opens an owner turn. D9 action requests journal the configured `d9_approval_timeout_secs` (300 seconds by default, clamped to 1–600). The gate and the app's approval countdown use the same stored `timeout_at_ms`.
