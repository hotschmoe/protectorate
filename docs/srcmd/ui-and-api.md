# API Schema, Flutter Web UI & Envoy MCP (`src/api/`, `src/tools/apigen.zig`, `src/cli/`)

## Single-Source Schema & Dart Codegen

`src/api/schema.zig` defines every wire enum (31), union (3), record struct (100+), and all 49 RPC methods (`methods`). `pt-apigen` (`src/tools/apigen.zig`) uses Zig comptime reflection to emit `app/lib/api/api.g.dart` directly into the Flutter project, and `zig build check-gen` fails if generated client bindings drift from the schema.

## Embedded Web UI & HTTP/WS Server (`src/hub/http_server.zig`, `src/tools/embed_web.zig`)

At build time `pt-embed-web` bundles `app/build/web` into a pure-Zig module (`web_assets`) embedded directly inside the `protectorate` binary. At runtime `std.http.Server` serves:
- `GET /health`: unauthenticated liveness check.
- `POST /api/<Method>`: bearer-authenticated RPC dispatch (`PairDevice` is public one-time claim; `.owner` and `.envoy_only` scopes checked per `schema.methods`).
- `GET /api/events`: RFC 6455 WebSocket stream (accepting `Authorization: Bearer` header or `?access_token=` query param for browsers), emitting `snapshot` + `live` + incremental `HubStreamMessage` updates from `StreamHub`.
- `GET /*`: embedded Flutter SPA assets.

## CLI & Envoy MCP (`src/cli/`)

`cli/commands.zig` provides the operator CLI (`serve`, `status`, `doctor`, `pair`, `token`, `sleeve`, `node`, `job`, `history`, `quota`, `rpc`). `protectorate --envoy` / `mcp` (`src/cli/mcp.zig`) serves JSON-RPC 2.0 over stdio exposing Envoy tools (`spawn_sleeve`, `send_message`, `get_transcript`, `wait_sleeve`, `run_job`, `search_history`, etc.).
