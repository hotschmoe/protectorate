package desk

import "testing"

var usLayout = layout{
	key: func(r rune) (uint16, bool, bool) {
		switch {
		case r >= 'a' && r <= 'z':
			return uint16(r - 'a' + 'A'), false, true
		case r >= 'A' && r <= 'Z':
			return uint16(r), true, true
		case r >= '0' && r <= '9':
			return uint16(r), false, true
		case r == ' ':
			return 0x20, false, true
		case r == '!':
			return '1', true, true
		case r == ':':
			return 0xBA, true, true
		}
		return 0, false, false
	},
	scan: func(vk uint16) uint16 { return vk + 0x100 },
}

func down(vk uint16) keyInput { return keyInput{typ: 1, vk: vk, scan: vk + 0x100} }
func up(vk uint16) keyInput   { return keyInput{typ: 1, vk: vk, scan: vk + 0x100, flags: keyUp} }

func TestTypeGroups(t *testing.T) {
	tests := []struct {
		name string
		text string
		want [][]keyInput
	}{
		{"lowercase", "a", [][]keyInput{{down('A'), up('A')}}},
		{"uppercase", "A", [][]keyInput{{down(0x10), down('A'), up('A'), up(0x10)}}},
		{"digit and space", "1 ", [][]keyInput{{down('1'), up('1')}, {down(0x20), up(0x20)}}},
		{"shifted symbols", "!:", [][]keyInput{{down(0x10), down('1'), up('1'), up(0x10)}, {down(0x10), down(0xBA), up(0xBA), up(0x10)}}},
		{"newline is enter, cr dropped", "\r\n", [][]keyInput{{down(0x0D), up(0x0D)}}},
		{"tab", "\t", [][]keyInput{{down(0x09), up(0x09)}}},
		{"char the layout lacks", "\u00e9", [][]keyInput{{
			{typ: 1, scan: 0xE9, flags: keyUnicode}, {typ: 1, scan: 0xE9, flags: keyUnicode | keyUp}}}},
		{"non-BMP U+1D400", "\U0001D400", [][]keyInput{
			{{typ: 1, scan: 0xD835, flags: keyUnicode}, {typ: 1, scan: 0xD835, flags: keyUnicode | keyUp}},
			{{typ: 1, scan: 0xDC00, flags: keyUnicode}, {typ: 1, scan: 0xDC00, flags: keyUnicode | keyUp}}}},
	}
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := typeGroups(tt.text, usLayout)
			if len(got) != len(tt.want) {
				t.Fatalf("got %d groups, want %d", len(got), len(tt.want))
			}
			for i := range got {
				if len(got[i]) != len(tt.want[i]) {
					t.Fatalf("group %d has %d events, want %d", i, len(got[i]), len(tt.want[i]))
				}
				for j := range got[i] {
					if got[i][j] != tt.want[i][j] {
						t.Errorf("group %d event %d = %+v, want %+v", i, j, got[i][j], tt.want[i][j])
					}
				}
			}
		})
	}
}
