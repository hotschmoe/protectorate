package hub

import (
	"os"
	"path/filepath"
	"testing"
)

func TestDeskUseCountsPerNodeAndCaller(t *testing.T) {
	var u deskUse
	u.add("pc", "owner")
	u.add("pc", "sleeve api")
	u.add("pc", "sleeve api")
	u.add("other", "owner")
	got := u.get("pc")
	if len(got) != 2 || got[0].Caller != "owner" || got[0].Count != 1 || got[1].Caller != "sleeve api" || got[1].Count != 2 {
		t.Errorf("pc = %v", got)
	}
	u.day = "1999-01-01"
	if g := u.get("pc"); len(g) != 0 {
		t.Errorf("counts survived the day change: %v", g)
	}
}

func TestDeskUseSurvivesRestartWithinTheDay(t *testing.T) {
	path := filepath.Join(t.TempDir(), "desk-use.json")
	var first deskUse
	if err := first.open(path); err != nil {
		t.Fatal(err)
	}
	first.add("pc", "owner")
	first.add("pc", "sleeve api")
	first.add("pc", "sleeve api")

	var second deskUse
	if err := second.open(path); err != nil {
		t.Fatal(err)
	}
	if got := second.get("pc"); len(got) != 2 || got[0].Count != 1 || got[1].Count != 2 {
		t.Fatalf("after restart = %v", got)
	}
	second.add("pc", "owner")
	if got := second.get("pc"); got[0].Count != 2 {
		t.Fatalf("counting resumes: %v", got)
	}

	if err := os.WriteFile(path, []byte(`{"day":"1999-01-01","counts":{"pc":{"owner":9}}}`), 0o600); err != nil {
		t.Fatal(err)
	}
	var yesterday deskUse
	if err := yesterday.open(path); err != nil {
		t.Fatal(err)
	}
	if got := yesterday.get("pc"); len(got) != 0 {
		t.Fatalf("yesterday's counts were adopted: %v", got)
	}
}
