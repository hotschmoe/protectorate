package main

import "testing"

func TestParseSize(t *testing.T) {
	tests := []struct {
		in      string
		want    uint64
		wantErr bool
	}{
		{"512", 512, false}, {"512M", 512 << 20, false}, {"8G", 8 << 30, false}, {"8GiB", 8 << 30, false},
		{"1.5g", 3 << 29, false}, {"2K", 2048, false}, {"", 0, true}, {"G", 0, true}, {"-1G", 0, true}, {"abc", 0, true},
	}
	for _, tt := range tests {
		got, err := parseSize(tt.in)
		if (err != nil) != tt.wantErr || got != tt.want {
			t.Errorf("parseSize(%q) = %d, %v", tt.in, got, err)
		}
	}
}
