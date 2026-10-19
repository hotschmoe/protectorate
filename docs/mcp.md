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
  `quota_get_quota`, `backlog_add_item`
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
protectorate mcp --http 127.0.0.1:7480
claude mcp add --transport http protectorate http://127.0.0.1:7480
```

The HTTP endpoint has no authentication of its own and acts with the bridge's token.
Keep it on loopback.

## Example tool list (`--allow '*'`)

```
backlog_add_item        mutating
backlog_list_items      read-only
backlog_update_item     mutating
hub_get_status          read-only
quota_get_quota         read-only
session_capture_pane    read-only
session_create_session  mutating
session_kill_session    mutating, destructive
session_list_sessions   read-only
```
