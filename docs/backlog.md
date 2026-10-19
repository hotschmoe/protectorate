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
  the request: `owner`, a sleeve name, `poe` (the orchestrator), `hub`, or
  `token:<name>` for another token

A note needs a title or a body. Lists sort pinned first, then most recently
updated.

## Storage

One file on the hub, `$PROTECTORATE_HOME/notes.json` (default
`~/.protectorate/notes.json`), written by temp file and rename under an OS file
lock (a `.notes.json.lock` sidecar), so concurrent hub and CLI writers never
lose each other's changes and a reader never sees a partial file. Back it up
with the rest of the hub home.

### Migration

A hub that still has the earlier markdown backlog converts it on first load:
every `- [ ] title  #p2 @codex id:ab12` item becomes a note with the same id,
tagged with its `## project` heading (source `owner`, both timestamps the
file's modification time). Indented lines become the start of the body;
issue link, priority, harness preference and claimed or done state are
appended to the body as plain lines (`Issue:`, `Priority:`, `Harness:`,
`Status:`). The old file is then renamed to `backlog.md.migrated` and never
read again. Messages that referenced an item id keep working because ids are
preserved.

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
`query`), `GetNote`, `CreateNote`, `UpdateNote` (only the fields that are set
change; `project: ""` clears the tag), `DeleteNote` (destructive, so the
orchestrator is not granted it by default). MCP tools: `backlog_list_notes`,
`backlog_get_note`, `backlog_create_note`, `backlog_update_note`,
`backlog_delete_note`. Scopes stay `backlog_*`: device pairing grants it, and
so do sleeve tokens. Mutations publish `note.created|updated|deleted` events
(subject = note id).

An update replaces the whole body it is given; two writers editing the same
note at the same moment, such as the app autosaving while the orchestrator
appends, resolve as last write wins.

## CLI

```
protectorate backlog add "Title" [--body text | --body-file path|-] [--project tag] [--pin]
protectorate backlog ls [--project tag] [--query text]
protectorate backlog show <id>
protectorate backlog edit <id> [--title t] [--body text | --body-file path|- | --append text] [--project tag]
protectorate backlog rm <id>
protectorate backlog pin <id> [--off]
```

`edit` with none of its flags opens the note in `$EDITOR` as the title, a blank
line and the body. All commands take `--json`.

## App

The BACKLOG tab is a notes view in the instrument style of docs/design.md: a
list (pinned first, then updated; project tag chips; "from <sleeve>" for notes
a sleeve or the hub filed) and an editor with a title field and a multi-line
body. Edits autosave after a short pause; a save that fails (offline, hub
restarting) stays dirty and retries until it lands. Delete asks for
confirmation. On a wide window the list and the editor sit side by side; on a
phone the editor opens over the list.
