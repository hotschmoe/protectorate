package desk

import "testing"

func TestTypeGroups(t *testing.T) {
	tests := []struct {
		name string
		text string
		want []keyInput
	}{
		{"ascii", "a:", []keyInput{
			{typ: 1, scan: 'a', flags: keyUnicode}, {typ: 1, scan: 'a', flags: keyUnicode | keyUp},
			{typ: 1, scan: ':', flags: keyUnicode}, {typ: 1, scan: ':', flags: keyUnicode | keyUp}}},
		{"newline is enter, cr dropped", "\r\n", []keyInput{
			{typ: 1, vk: 0x0D}, {typ: 1, vk: 0x0D, flags: keyUp}}},
		{"tab", "\t", []keyInput{{typ: 1, vk: 0x09}, {typ: 1, vk: 0x09, flags: keyUp}}},
		{"non-BMP U+1D400", "\U0001D400", []keyInput{
			{typ: 1, scan: 0xD835, flags: keyUnicode}, {typ: 1, scan: 0xD835, flags: keyUnicode | keyUp},
			{typ: 1, scan: 0xDC00, flags: keyUnicode}, {typ: 1, scan: 0xDC00, flags: keyUnicode | keyUp}}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			var got []keyInput
			for _, g := range typeGroups(tt.text) {
				if len(g) != 2 {
					t.Fatalf("group of %d events, want a down/up pair", len(g))
				}
				got = append(got, g...)
			}
			if len(got) != len(tt.want) {
				t.Fatalf("got %d events, want %d", len(got), len(tt.want))
			}
			for i := range got {
				if got[i] != tt.want[i] {
					t.Errorf("event %d = %+v, want %+v", i, got[i], tt.want[i])
				}
			}
		})
	}
}

func TestTypeGroupsDistinct(t *testing.T) {
	gs := typeGroups("desk test:")
	seen := map[uint16]bool{}
	for _, g := range gs {
		seen[g[0].scan] = true
	}
	if len(seen) != 7 {
		t.Errorf("distinct scans = %d, want 7 (d e s k space t :)", len(seen))
	}
}
