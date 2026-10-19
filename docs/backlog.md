# Backlog

Low-priority work for spare subscription capacity (PROBLEMS.md P6). One
central markdown file on the hub: `$PROTECTORATE_HOME/backlog.md` (default
`~/.protectorate/backlog.md`). Edit it in any editor (`protectorate backlog
edit`) or through the CLI/API; both coexist.

```
## hotschmoe/protectorate
- [ ] Add GPU metrics to node agent  #p2 @any  id:b7f3
  Optional indented notes.
- [~] Fix flaky test  https://github.com/hotschmoe/protectorate/issues/12  @codex  id:9c1a claim:alice/codex@2026-10-19T10:00:00Z
- [x] Done item  id:0c2d
## donate: owner/public-repo
- [ ] https://github.com/owner/public-repo/issues/345  id:41ee
```

## Format

- `## <heading>` starts a project: `owner/repo` or a local path. A heading
  starting with `donate:` marks a public repo we do not own.
- An item is a line starting `- [ ]` (todo), `- [~]` (claimed) or `- [x]`
  (done) at column 0. Indented lines after it are its notes.
- Metadata are words at the END of the item line; the title is everything
  before them. Each kind is recognized once, scanning from the right:
  - `https://...` issue or PR link (linked, never copied)
  - `#p1`..`#p9` priority (1 highest)
  - `@claude|@codex|@grok|@pi|@any` harness preference (unset = any)
  - `id:<short>` stable id, appended by the hub to items lacking one
  - `claim:<by>@<RFC3339>` who took it (`<sleeve>` or `<sleeve>/<harness>`)
- Everything else (free text, other headings, other bullets) is preserved
  verbatim, in order. Line endings and a missing final newline are kept.

## Semantics

- Claiming is the dispatcher's mutex: it succeeds only on a todo item (or is a
  no-op for the same claimant), so two dispatchers never take the same item.
  Done keeps the claim token, recording which harness attempted it.
- Release (back to todo) and reopen clear the claim.
- Harness filter: `--harness codex` returns `@codex`, `@any` and unset items.
- Donate items require an issue URL. Donated work must land in a review inbox
  (a branch on our fork plus a summary) and is never posted publicly without
  user approval. Own-repo work may open draft PRs, never merge.

## Writes

Every change re-reads the file, edits only the touched line, writes a temp file
and renames it over the original. A lock file serializes hub processes. If the
file changed between read and rename (a human saving), the change is re-applied
to the new content. A human save landing in the microseconds between the final
check and the rename can still be lost; the lock cannot cover editors.

## API

`BacklogService`: `ListItems`, `AddItem`, `UpdateItem`. Mutations publish
`backlog.added|claimed|released|done|reopened` events (subject = item id).
CLI: `backlog ls|add|claim|done|edit`, all with `--json` (`edit --json` prints
the file path instead of opening the editor).
