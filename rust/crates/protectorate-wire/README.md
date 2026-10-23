# Protectorate wire foundation

The embedded descriptor is generated from `proto/protectorate/v1` and its imports:

```sh
rust/crates/protectorate-wire/scripts/update-descriptor.sh
rust/crates/protectorate-wire/scripts/update-descriptor.sh --check
```

`Catalog::embedded()` resolves procedure paths, message descriptors, canonical
CLI tool names, idempotency, streaming flags and the schema's owner/device/node
annotations. No Rust service definition duplicates the protobuf API.

`decode_json` and `encode_json` implement protobuf JSON using prost-reflect.
Requests reject unknown fields (including nested fields), duplicate field
aliases, conflicting oneof members, empty payloads and trailing JSON. Standard
protobuf integer, enum, bytes, timestamp and duration representations are used.

`UnaryServer` takes a mandatory authorization callback and async registered
handlers. The callback receives method annotations and HTTP headers and returns
an `AuthContext`; the application supplies its token/scope/device policy.
Handlers receive a decoded `DynamicMessage` and return a `RpcResponse` of the
method's declared output type. `.router()` returns an Axum router suitable for
`axum::serve` or nesting in the hub HTTP server.

Implemented unary transport: Connect POST, `application/json`, optional protocol
version 1 header, canonical JSON errors and HTTP statuses, response metadata,
request body limits, and `connect-timeout-ms` deadlines that drop handler futures.

`register_streaming` registers a server-only streaming producer taking
`(RpcRequest, StreamSender)` and returning `Result<StreamTrailers>`. Call
`sender.send(message).await` for each response, and return an empty trailer map
for successful completion. Request/response envelopes use the Connect five-byte
header and `application/connect+json`; every response ends with a JSON end-stream
frame, including authentication, decoding, producer and deadline errors. Trailer
metadata is represented as string arrays in that final frame.

The producer channel is bounded (eight messages by default, configurable), so
awaited sends apply backpressure instead of growing without limit. Body drop
aborts the producer and signals cloned senders. `sender.cancel()` lets token
revocation or other application policy terminate an active stream; producers and
child tasks can observe `is_cancelled()` or `cancelled().await`. Deadlines apply
to the full stream. Handlers must return or yield normally for cancellation to
interrupt them; blocking work belongs on an appropriate runtime executor.
The repository Flutter client selects Connect's JSON codec and unary POST;
the tests exercise that wire shape through real TCP HTTP, without requiring Dart.

Not implemented here: business service handlers; client/bidirectional streams;
WebSocket terminals; Connect GET; protobuf binary/gRPC/gRPC-web; compression;
CORS policy; and HTTP/2/TLS listener configuration. Server-stream leading metadata
is transport-owned; application trailing metadata is supported. The real HTTP
tests decode Connect framing, including errors and trailers, but do not execute
the Dart runtime. See the [Connect protocol](https://connectrpc.com/docs/protocol/).
