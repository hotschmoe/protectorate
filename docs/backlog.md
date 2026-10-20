# Notes (backlog)

For now the backlog is a plain notes app. The owner writes notes in the app or
the CLI; they live on the hub, and the orchestrator sleeve reads and edits them
with the same CLI or MCP tools, so a note can be something to talk about with
it. The earlier dispatcher idea (claim an item, run it on spare subscription
capacity, PROBLEMS.md P6) is not built; notes are the part that is.

## Model

A note has:

- `id`: short hex, stable
- `title`: one line, up to 200 characters; may be empty when there is a body
- `body`: markdown text, up to 256 KiB
- `created_at`, `updated_at`: the second moves only when something changed
- `pinned`: pinned notes list first
- `project`: optional tag, e.g. `hotschmoe/protectorate`
- `source`: who created it, taken from the caller's identity and never from
  the request: `owner`, a sleeve name, `envoy` (the orchestrator), `hub`, or
  `token:<name>` for another token
- `state`: `open` or `done`; lists show open notes by default
- `done_at`, `done_by`, `resolution`: when it was closed, the actor as named
  in the audit log (`owner` or `token:<name>`, including `token:sleeve:<name>`),
  and an optional single line of up to 200 characters explaining completion.
  The API groups these three fields under `completion`; storage keeps them
  directly on the note. Open notes have no completion fields.

A note needs a title or a body. Lists sort pinned first, then most recently
updated.

Closing an already done note preserves its completion time and actor; an
explicit resolution can replace the line. Reopening clears all completion
fields. Editing the title, body, tag or pin preserves the state. Completion
records the latest close; earlier changes remain in the audit log.

## Storage

One file on the hub, `$PROTECTORATE_HOME/notes.json` (default
`~/.protectorate/notes.json`), written by temp file and rename under an OS file
lock (a `.notes.json.lock` sidecar), so concurrent hub and CLI writers never
lose each other's changes and a reader never sees a partial file. Back it up
with the rest of the hub home.

Existing notes without a state load as open. No migration command is needed.

The hub reads only `notes.json`. The pre-notes markdown backlog converter has
been removed; an installation still using that retired format must convert
its notes before upgrading.

## What files notes by itself

- A mailbox `request` to a project with no live sleeve becomes a note tagged
  with the project: title `<sender> asks: <first line>`, body the full request
  plus its thread and message ids, source the sender. The message and its
  delivery refer to the note id (`note_id`), and the sleeve that later picks
  the request up is told to work from that note.
- A dependency notice for a project nobody works on becomes a note (source
  `hub`, once per change) whose body is the same facts a message would carry.

## API

`BacklogService`: `ListNotes` (filter by `project` and a case-insensitive
`query`, and `state`: omitted or `open`, `done`, `all`), `GetNote`,
`CreateNote`, `UpdateNote` (only the fields that are set change;
`project: ""` clears the tag; `state: "done"` closes with an optional
`resolution`, `state: "open"` reopens), `DeleteNote` (destructive, so the
orchestrator is not granted it by default). MCP tools: `backlog_list_notes`,
`backlog_get_note`, `backlog_create_note`, `backlog_update_note`,
`backlog_delete_note`. Scopes stay `backlog_*`: device pairing grants it, and
so do sleeve tokens. Mutations publish `note.created|updated|deleted` events
(subject = note id, payload includes state). Closing and reopening use the
existing `backlog_update_note` scope, with no creator or owner restriction:
workers may close their own tasks or other completed notes. A `read` token
alone cannot change state. The actor and time are set by the hub, never by
the request. A resolution can only be set on a done note.

For example, MCP `backlog_update_note` arguments
`{"id":"0a94","state":"done","resolution":"implemented in 8b98d58"}`
close the note. `backlog_list_notes` with `{}` lists open work; use
`{"state":"done"}` or `{"state":"all"}` for history.

An update replaces the whole body it is given; two writers editing the same
note at the same moment, such as the app autosaving while the orchestrator
appends, resolve as last write wins.

## CLI

```
protectorate backlog add "Title" [--body text | --body-file path|-] [--project tag] [--pin]
protectorate backlog ls [--project tag] [--query text] [--all | --state open|done|all]
protectorate backlog show <id>
protectorate backlog edit <id> [--title t] [--body text | --body-file path|- | --append text] [--project tag]
protectorate backlog rm <id>
protectorate backlog pin <id> [--off]
protectorate backlog done <id> [--resolution "implemented in 8b98d58"]
protectorate backlog reopen <id>
```

`edit` with none of its flags opens the note in `$EDITOR` as the title, a blank
line and the body. All commands take `--json`.

## App

The NOTES tab is a notes view in the instrument style of docs/design.md: a
list (pinned first, then updated; project tag chips; "from <sleeve>" for notes
a sleeve or the hub filed) and an editor with a title field and a multi-line
body. Edits autosave after a short pause; a save that fails (offline, hub
restarting) stays dirty and retries until it lands. Delete asks for
confirmation. On a wide window the list and the editor sit side by side; on a
phone the editor opens over the list. Open notes show by default; SHOW DONE
includes completed notes, with their resolution. The editor offers DONE with
an optional resolution line, or REOPEN for a done note, and shows the
completion actor and time. State actions first save pending edits.
