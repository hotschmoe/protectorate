package e2e

import "reflect"

type noteSnapshot struct {
	notes   []map[string]any
	deleted string
}

func (c *cluster) noteCRUD() noteSnapshot {
	c.t.Helper()
	create := func(title string, pinned bool) map[string]any {
		return c.call("BacklogService", "CreateNote", map[string]any{"title": title, "body": "initial body", "project": "e2e/notes", "pinned": pinned})["note"].(map[string]any)
	}
	edited := create("editable note", false)
	originalCreated := edited["createdAt"]
	edited = c.call("BacklogService", "UpdateNote", map[string]any{"id": edited["id"], "title": "persisted title", "body": "edited markdown\n\n**preserve this**", "pinned": true, "project": "e2e/notes"})["note"].(map[string]any)
	if edited["createdAt"] != originalCreated || edited["body"] != "edited markdown\n\n**preserve this**" || edited["title"] != "persisted title" || edited["pinned"] != true || edited["source"] != "owner" {
		c.t.Fatalf("updated note lost fields or identity: %v", edited)
	}
	unpinned := create("unpinned note", true)
	unpinned = c.call("BacklogService", "UpdateNote", map[string]any{"id": unpinned["id"], "pinned": false, "project": ""})["note"].(map[string]any)
	if unpinned["pinned"] == true || unpinned["project"] != nil && unpinned["project"] != "" {
		c.t.Fatalf("optional false/empty updates ignored: %v", unpinned)
	}
	deleted := create("must stay deleted", false)["id"].(string)
	c.call("BacklogService", "DeleteNote", map[string]any{"id": deleted})
	snapshot := noteSnapshot{notes: []map[string]any{edited, unpinned}, deleted: deleted}
	c.checkNotes(snapshot)
	return snapshot
}

func (c *cluster) checkNotes(snapshot noteSnapshot) {
	c.t.Helper()
	for _, want := range snapshot.notes {
		got := c.call("BacklogService", "GetNote", map[string]any{"id": want["id"]})["note"]
		if !reflect.DeepEqual(got, want) {
			c.t.Fatalf("persisted note changed: got=%v want=%v", got, want)
		}
	}
	var missing map[string]any
	if status := c.rpc("BacklogService", "GetNote", map[string]any{"id": snapshot.deleted}, &missing); status != 404 || missing["code"] != "not_found" {
		c.t.Fatalf("deleted note returned: status=%d response=%v", status, missing)
	}
	filtered := c.call("BacklogService", "ListNotes", map[string]any{"project": "e2e/notes", "query": "PERSISTED TITLE"})
	notes, ok := filtered["notes"].([]any)
	if !ok || len(notes) != 1 || !reflect.DeepEqual(notes[0], snapshot.notes[0]) {
		c.t.Fatalf("persisted project/query filter: %v", filtered)
	}
	all := c.call("BacklogService", "ListNotes", map[string]any{})["notes"].([]any)
	if len(all) < 2 || !reflect.DeepEqual(all[0], snapshot.notes[0]) {
		c.t.Fatalf("pinned note lost priority: %v", all)
	}
}
