# MCP server

`protectorate mcp` serves the hub API as MCP tools so an AI client can act as,
or talk to, the orchestrator. It is a client of the hub: it reads the hub token
the same way the CLI does and calls the normal Connect API (`--hub` or
`PROTECTORATE_HUB`, default `http://127.0.0.1:7470`).

## Tools come from the proto

Nothing is hand-listed. At startup the bridge reads an embedded buf image
(`internal/mcpbridge/image.binpb`, written by `make gen`) and turns every unary
RPC into a tool:

- name: `<service>_<method>` in snake_case without the `Service` suffix, e.g.
  `quota_get_quota`, `backlog_create_note`
- description: the RPC's proto comment; field comments become property
  descriptions
- input schema: derived from the request message (enums as string enums,
  repeated, maps, nested messages, Timestamp, Duration, Struct)
- call: arguments are parsed as strict proto-JSON (unknown fields are an error),
  sent to the hub, and the response comes back as proto-JSON text
- errors: returned as tool errors whose text starts with the Connect code, e.g.
  `not_found: ...`

A new RPC in `proto/` appears as a tool after `make gen`. A test fails if an RPC
has no comment or if the image is out of date with the generated code.
Server-streaming RPCs (`EventService.Subscribe`) are not exposed; use
`protectorate events` for those.

## Scoping

An RPC marked `option idempotency_level = NO_SIDE_EFFECTS` is read-only; all
others are mutating and carry `readOnlyHint=false`. `(protectorate.v1.destructive)
= true` additionally sets `destructiveHint` (for example `KillSession`).

- No flags: read-only tools only.
- `--allow GLOB` (repeatable) replaces that default with globs over tool names.
  `--allow '*'` serves everything.
- `--deny GLOB` (repeatable) removes matches and always wins.

```
protectorate mcp                                   # read-only
protectorate mcp --allow 'backlog_*' --allow 'quota_*'
protectorate mcp --allow '*' --deny 'session_kill_*'
```

These filters only limit what the client is offered. What a bridge can actually
do is set by the token it runs with, which the hub enforces. Give every bridge
a scoped token (see [auth.md](auth.md)):

```
protectorate token create claude-bridge --scope read --scope 'backlog_*' --ttl 720h
protectorate mcp --token-file ~/.protectorate/claude-bridge.token --allow 'backlog_*'
```

The token comes from `--token-file`, then env `PROTECTORATE_TOKEN`, then the
owner token file. Scopes use the same tool names as this bridge. TokenService
is owner-only and is never exposed as a tool.

## Registering

stdio (default):

```
claude mcp add protectorate -- protectorate mcp
claude mcp add protectorate -- protectorate mcp --allow 'backlog_*'
codex mcp add protectorate -- protectorate mcp
```

Codex can also be configured in `~/.codex/config.toml`:

```
[mcp_servers.protectorate]
command = "protectorate"
args = ["mcp"]
```

Grok and pi: use their MCP server configuration with command `protectorate`
and args `["mcp"]` (the common `mcpServers` JSON shape). These two were not
verified against a live client.

Streamable HTTP, for bridges that cannot spawn a process:

```
protectorate mcp --http 127.0.0.1:7471
claude mcp add --transport http protectorate http://127.0.0.1:7471 \
  --header "Authorization: Bearer $(cat $PROTECTORATE_HOME/token)"
```

Every HTTP request must carry the bridge's own hub token as a bearer, or it
gets 401. The endpoint acts with that token, so only a caller who already holds
it can use it; this reuses the existing token machinery instead of adding a
second secret. Requiring the bearer was chosen over only refusing non-loopback
binds because a loopback bind still lets any local process or user, and any
web page that can reach localhost, drive the bridge. Because the bearer is sent
in cleartext over plain HTTP, `--http` accepts only loopback and tailnet IPs
(100.64.0.0/10, fd7a:115c:a1e0::/48), where WireGuard encrypts it; any other
address, including an empty host, is refused.

HTTP bridges retain at most 128 sessions. Clients should close their session
with `DELETE` when finished; a full bridge returns HTTP 503 for a new session
while existing clients keep working. Sessions idle for an hour are expired
on the next request. Active requests are allowed to finish.
Each session admits at most 128 requests in progress, on stdio and HTTP;
excess requests receive JSON-RPC error -32000. Cancellation notifications
remain available at capacity and release slots when their calls finish.

## Example tool list (`--allow '*'`)

`protectorate mcp --list` prints the catalog the bridge would serve, with the
same `--allow`/`--deny` filters, as JSON and exits without contacting the hub.
`make gen` writes `tests/e2e/testdata/mcp_tools.json` from it, and the e2e
suite checks the served `tools/list` against that file, so a schema change
updates the golden like any other generated output.

```
backlog_create_note     mutating
backlog_delete_note     mutating, destructive
backlog_get_note        read-only
backlog_list_notes      read-only
backlog_update_note     mutating
hub_get_status          read-only
quota_get_quota         read-only
quota_suggest           read-only
session_capture_pane    read-only
session_kill_session    mutating, destructive
session_list_sessions   read-only
```
