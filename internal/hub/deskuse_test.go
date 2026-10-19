package hub

import "testing"

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
