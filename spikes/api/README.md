# Spike 4: Connect-RPC vs plain JSON-RPC

Same tiny API built twice: `SleeveService{List, Spawn}` and `EventService.Subscribe` (server stream).

- `connect/` - A: proto + buf + connect-go + connectrpc (Dart). Server :7471.
- `jsonrpc/` - B: hand-written Go structs, JSON-RPC 2.0 at `POST /rpc`, NDJSON stream at `GET /events`, hand-written Dart client on `package:http`. Server :7472.

Both run: Go server, Go client (A only), Dart console client (both) doing Spawn, duplicate Spawn (error), List, and consuming the stream. Both emit an MCP tool list at `GET /mcp/tools`.

## Reproduce

```
# A
cd connect
buf generate && buf build -o server/image.binpb
go run ./server &            # :7471
go run ./client
cd dart && dart pub get && dart run bin/main.dart [--binary]
bash curl.sh
# B
cd jsonrpc
go run ./server &            # :7472
cd dart && dart pub get && dart run bin/main.dart
bash curl.sh
```

Tool installs for A: `go install` of buf, protoc-gen-go, protoc-gen-connect-go; `dart pub global activate protoc_plugin` and `dart pub global activate connectrpc` (provides `protoc-gen-connect-dart`). B needs none.

Note: `connect/dart/lib/gen/` holds stale untracked output from my first `buf generate` run (it used the `grpc` option); it is not committed and is excluded in `analysis_options.yaml`. It can be deleted by a human; I did not delete it per house rules.

## Comparison

| | A: Connect | B: JSON-RPC + NDJSON |
|---|---|---|
| Hand-written lines (schema + Go server + Go client + Dart client/demo, incl. business logic and MCP) | 364: proto 61, server 101, mcp.go 103, Go client 40, Dart demo 44, buf yaml 15 | 381: server 132 (types + logic), rpc.go 150 (dispatch + schema reflection), Dart client 73, Dart demo 26. No Go client written (~50 more to reach parity) |
| Generated lines | 801 Go + 995 Dart | 0 |
| Go deps | 2 direct (connect-go, protobuf), go.sum 6 lines, `go list -m all` = 5 | stdlib only, no go.sum |
| Dart deps (pubspec.lock) | 7 packages (connectrpc, protobuf, http2, fixnum, path, collection, meta) | 11 packages (http pulls async, http_parser, source_span, ...) |
| Codegen steps | `buf generate` (4 local plugins) + `buf build -o image.binpb` (for MCP descriptions) | none |
| Tools to install | buf, protoc-gen-go, protoc-gen-connect-go, protoc_plugin, connectrpc generator | none |
| Go server binary (-s -w) | 8.78 MB | 6.23 MB (A is +2.55 MB, +41%) |
| Go server binary (default) | 12.88 MB | 9.19 MB (+3.7 MB) |
| Unary curl | clean (see below) | clean (see below) |
| Stream curl | needs 5-byte envelope prefix and shows binary framing | `curl -N` just works, readable lines |
| HTTP status on error | proper (409, 400, ...) with `{code,message}` | always 200; error in body |
| Error model | 16 canonical codes, typed details, `connect.CodeOf(err)`, Dart `ConnectException.code` enum | ad-hoc int codes; invented by us in both Go and Dart |
| Unknown request field (`{"nme":..}`) | silently ignored (proto JSON default) | rejected (`DisallowUnknownFields`) |
| Required fields in MCP schema | not expressible in proto3 without extra annotations | free from `omitempty` absence |
| MCP tool JSON | from descriptors; descriptions need source info (see rough edges) | from struct reflection + `desc` tag; same registry also does dispatch |
| Schema change propagation | edit .proto, `buf generate`; Go + Dart typed fields appear, nothing breaks; `buf breaking` can guard renames | edit Go struct; server and MCP follow automatically; every Dart (and Go) client needs a manual edit, nothing flags it |
| Dart streaming over HTTP/1.1 | works, incremental (heartbeats at 1s) | works, incremental |
| int64 | JSON string on wire, `Int64` in Dart | plain number (53-bit precision limit in JS/web) |
| Binary wire | `--binary` flag, one line change | no |

## Evidence

### curl (A)

```
curl -H 'Content-Type: application/json' -d '{"name":"alice","harness":"claude"}' \
  http://127.0.0.1:7471/protectorate.v1.SleeveService/Spawn
-> {"sleeve":{"id":"sl-1","name":"alice","harness":"claude","state":"running"}}

curl -i -H 'Content-Type: application/json' -d '{"name":"alice"}' .../Spawn      # duplicate
-> HTTP/1.1 409 Conflict   {"code":"already_exists","message":"sleeve \"alice\" exists"}

curl 'http://127.0.0.1:7471/protectorate.v1.SleeveService/List?encoding=json&message=%7B%7D'
-> works only after marking List `idempotency_level = NO_SIDE_EFFECTS` (405 before)

printf '\000\000\000\000\002{}' | curl -sN -H 'Content-Type: application/connect+json' \
  --data-binary @- http://127.0.0.1:7471/protectorate.v1.EventService/Subscribe
-> binary-prefixed frames: ....{"type":"heartbeat"}....{"type":"heartbeat"}
```

### curl (B)

```
curl -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"sleeve.spawn","params":{"name":"alice","harness":"claude"}}' \
  http://127.0.0.1:7472/rpc
-> {"jsonrpc":"2.0","id":1,"result":{"sleeve":{...}}}

... "params":{"name":"alice"} (duplicate)
-> HTTP 200 {"jsonrpc":"2.0","id":2,"error":{"code":-32001,"message":"sleeve \"alice\" exists"}}

curl -N http://127.0.0.1:7472/events
-> {"seq":0,"type":"heartbeat"}
   {"seq":0,"type":"heartbeat"}
```

### MCP tool JSON for Spawn

A (from proto descriptors, via `buf build` image with source info):

```json
{
  "name": "sleeve_spawn",
  "description": "Spawn creates a sleeve running the given harness.",
  "inputSchema": {
    "type": "object",
    "additionalProperties": false,
    "properties": {
      "harness": {"type": "string", "description": "Agent harness to run: claude, gemini, opencode."},
      "name": {"type": "string", "description": "Human-readable sleeve name, unique per hub."}
    }
  }
}
```

B (struct reflection; same plus `"required": ["name"]`):

```json
{
  "name": "sleeve_spawn",
  "description": "Spawn creates a sleeve running the given harness.",
  "inputSchema": {
    "type": "object",
    "additionalProperties": false,
    "properties": {
      "harness": {"type": "string", "description": "Agent harness to run: claude, gemini, opencode."},
      "name": {"type": "string", "description": "Human-readable sleeve name, unique per hub."}
    },
    "required": ["name"]
  }
}
```

### Schema change experiment

Added `string workdir = 3` to `SpawnRequest` (A). `buf generate`: Go and Dart gained the field; `go build` and `dart analyze` stayed clean. The running server's `/mcp/tools` did not show it until `buf build -o image.binpb` was re-run and the server rebuilt. This is a silent-staleness trap; fold the image step into `make gen`, or use the Go descriptor registry and accept empty descriptions. For B, adding the field to `SpawnParams` updates server validation and MCP immediately, but Dart `spawn()` must be edited by hand and nothing reports the drift.

### Dart streaming over HTTP/1.1

Both clients printed events incrementally (`[345ms] sleeve.spawned`, `[1101ms] heartbeat`, `[2101ms] heartbeat`), so neither is buffering. connectrpc exposes this through `connectrpc/io.dart` (`createHttpClient(io.HttpClient())`, doc: HTTP/1 only, no trailers); `http2.dart` and `web.dart` (fetch) are separate entry points. Server streaming works on all three; bidi needs HTTP/2.

## Rough edges: connect-dart

- connectrpc 2.0.0 on pub.dev (published 2026-08-31; 1.0.0 was 2025-07-01; 9 versions since Dec 2024). Publisher `connectrpc.com`, 150/160 pub points, about 43k downloads/30d, 18 likes. Small community versus grpc-dart; the maintainers are the Connect org, so protocol conformance is credible.
- Platforms tagged: android, ios, linux, macos, windows, web. Web uses fetch (supports server streaming, no bidi). **Not verified here** in a real Flutter app (no desktop toolchain); only `dart run` on Linux/arm64.
- Needs two Dart plugins (`protoc-gen-dart` from `protoc_plugin` 25.1.0 plus `protoc-gen-connect-dart`). Do not pass the `grpc` option to protoc-gen-dart; it emits `*.pbgrpc.dart` that need `package:grpc` and fail analysis. Also emits unneeded `pbserver`/`pbjson`/`pbenum` files.
- Transport wiring is manual (`protocol.Transport(baseUrl, codec, httpClient)`) and spread over four imports (`connect.dart`, `io.dart`, `protobuf.dart`, `protocol/connect.dart`). Not hard, but not discoverable. The README quick start is the only guide.
- Errors: clean `ConnectException` with `Code` enum, message, details, metadata.
- Generated client is an `extension type`, with per-call headers, abort signal, and header/trailer callbacks. Interceptors exist.
- No `package:http` dependency; the HTTP/1 path is a thin wrapper over `dart:io.HttpClient`, so there is little transitive risk.

## Rough edges: Connect on the Go server

- Unknown JSON fields are dropped silently (typo'd `nme` produced a misleading "name is required").
- GET requires `idempotency_level` in the proto.
- Go registry descriptors have no source info, so MCP descriptions need an embedded image (extra step above).
- proto3 has no `required`; MCP schemas lose that unless we add `buf.validate` or a custom option.
- Server streams over curl need envelope framing; debuggability for streams is worse than NDJSON.

## Recommendation: keep Connect (with a cheap escape hatch)

Ethos note: "lightweight" applies to how we interact with AI harnesses, not to the hub API. The hub API should be correct, robust and fast, so the criteria that matter are type safety across Go/Dart, schema evolution, streaming reliability and tooling robustness. Line counts, dependency counts and binary size (where B wins) are secondary. On the criteria that matter, A wins: generated types on both sides, `buf breaking` for evolution, canonical error codes with HTTP statuses, and a spec-defined stream framing with end-of-stream error trailers (NDJSON has no standard way to signal a mid-stream error or clean end, and no resume/seq contract). B's only robustness edges are rejecting unknown fields (fixable in A with a custom codec or buf.validate) and `required` in MCP schemas (fixable with annotations). Conclusion: keep Connect; the case is stronger under this ethos.

At two methods the hand-written line counts are a wash (364 vs 381), and B is lighter on tooling, binary size (-2.5 MB stripped), and curl ergonomics. But B's Dart cost is O(methods + types): every message needs a hand-written class, every method a hand-written wrapper, and nothing flags drift between Go and Dart. Connect's cost is constant after the one-time setup, gives proper HTTP statuses and error codes, a binary mode, and `buf breaking` guarding schema evolution. With the stated surface (CLI, Flutter app, MCP, multi-node hub-to-node calls) one schema feeding all of them is worth +2.5 MB and five installed generators, all of which are build-time only.

Mitigations to adopt:

1. Add a `make gen` that runs `buf generate` and `buf build -o image.binpb` together (the one silent-staleness trap found).
2. Mark read-only methods `NO_SIDE_EFFECTS` so they are GET-curlable.
3. Escape hatch for connect-dart immaturity: the Connect unary protocol is plain `POST` + JSON and the server-stream framing is a 5-byte envelope, so a Flutter app can fall back to `package:http` against the same server without any server change. If connectrpc fails in the real Flutter app (web or mobile), do that for the few calls needed, rather than rewriting the API as JSON-RPC.
4. Verify connectrpc in an actual Flutter app (web + Android) early; this spike could not.
5. Consider keeping the MCP tool generator as the tiny descriptor walker here (103 lines) rather than adding a dependency.

Switch to JSON-RPC only if the Flutter verification in (4) fails and the hand-fallback in (3) proves to cover more than a handful of calls, or if the API stays under roughly 10 methods and Dart is the sole non-Go client.
