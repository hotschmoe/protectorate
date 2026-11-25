# API Schema, Flutter Web UI & Envoy MCP (`src/api/`, `src/tools/apigen.zig`, `src/cli/`)

## Single-Source Schema & Dart Codegen

`src/api/schema.zig` defines the wire types and RPC methods (`methods`). `pt-apigen` (`src/tools/apigen.zig`) uses Zig comptime reflection to emit `app/lib/api/api.g.dart` directly into the Flutter project, and `zig build check-gen` fails if generated client bindings drift from the schema.

`ListModels` returns a row for each harness, or one row when the request includes `harness`.
Each row contains model IDs and labels, optional provider IDs, default flags, and reasoning
effort choices. A model can advertise its own effort choices and default. An empty model list
keeps manual entry available. Each harness has a 60-second cache and at most one refresh
in flight. Concurrent requests can use the last successful snapshot or the initial fallback
while a probe runs. A failed refresh keeps the previous snapshot. A request for all harnesses
refreshes expired rows concurrently.

The app shares a model and effort picker across fixed-harness sleeve creation, Envoy setup,
and resleeve. Auto spawns let the quota selector choose the launch target. The picker
starts on the target harness defaults and offers manual model IDs. Pi catalog
entries carry their provider, and manual Pi entries accept an optional provider. The command
bar accepts `/resleeve <harness> [model] [effort]`. For Pi, a command can use `provider/model`
in its model slot. The picker sends empty selection fields on resleeve to request native
defaults. If a CLI or API caller omits a field on resleeve, it inherits that field from the
target harness's own preferences when available and uses the native default otherwise.

## Embedded Web UI & HTTP/WS Server (`src/hub/http_server.zig`, `src/tools/embed_web.zig`)

At build time `pt-embed-web` pre-compresses `app/build/web` files with gzip (`std.compress.flate.Compress`, keeping raw bytes only when compression does not shrink the asset) into the pure-Zig module `web_assets` embedded directly inside `protectorate`. Release builds (`zig build release-all` or `-Dstrip=true`) strip debug symbols while preserving `ReleaseSafe` checks. At runtime the hub's HTTP loop (docs/srcmd/network.md, "HTTP and WebSocket loop") serves:
- `GET /health`: unauthenticated liveness check.
- `POST /api/<Method>`: bearer-authenticated RPC dispatch (`PairDevice` is public one-time claim; `.owner` and `.envoy_only` scopes checked per `schema.methods`).
- `GET /api/events`: RFC 6455 WebSocket stream (accepting `Authorization: Bearer` header or `?access_token=` query param for browsers), emitting `snapshot` + `live` + incremental `HubStreamMessage` updates from `StreamHub`.
- `GET /*`: embedded Flutter SPA assets (serving pre-gzipped assets directly with `Content-Encoding: gzip` when accepted, or transparently inflating via `std.compress.flate.Decompress` for uncompressed clients).

## CLI & Envoy MCP (`src/cli/`)

`cli/commands.zig` provides the operator CLI (`serve`, `status`, `doctor`, `pair`, `token`, `quota`, `sleeve list|attach|stop|rm|prompt|transcript`, `node list|join-token|add|approve|update`, `hub update`, `harness update`, `job list|get|cancel|logs`, `history search`, `rpc`). All query and status commands render aligned human-readable tables by default and emit machine JSON with `--json`. `sleeve attach <id>` invokes `TakeoverTerminal` to release the hub's managed driver and hands off directly to the interactive harness CLI in the user's terminal.

`sleeve models [harness]` prints the catalog. `sleeve spawn` and `sleeve resleeve` take
`--model`, `--provider`, and `--effort`. On resleeve, an empty argument such as `--effort ''`
clears that field. The native provider options are documented in [Harness Drivers](drivers-and-sandbox.md).

`hub/api_dispatch.zig` and `hub/runtime.zig` enforce D9 owner-approval gating on Envoy action calls (`SpawnSleeve` past `d9_spawn_limit`, `RemoveSleeve`, `GitPush`, `RunJob`), block `EnvoyWait` on turn settlement and extract the Envoy transcript reply, and orchestrate fleet rollouts via `UpdateNodes` and harness updates via `UpdateHarnesses`. `protectorate --envoy` / `mcp` (`src/cli/mcp.zig`) serves JSON-RPC 2.0 over stdio exposing Envoy tools (`spawn_sleeve`, `send_message`, `get_transcript`, `wait_sleeve`, `run_job`, `search_history`, etc.).

`EnvoySend` queues vendor mail when the caller supplies an agent name or uses the envoy-only credential. Envoy sees `[from agent:<name>]` and the resulting turn is subject to D9. An unnamed send from the owner or a paired device opens an owner turn. D9 action requests journal the configured `d9_approval_timeout_secs` (300 seconds by default, clamped to 1–600). The gate and the app's approval countdown use the same stored `timeout_at_ms`.
