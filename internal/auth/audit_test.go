package auth

import (
	"fmt"
	"testing"
	"time"
)

func TestTailBoundsAcrossFiles(t *testing.T) {
	dir := t.TempDir()
	a := NewAudit(dir)
	defer a.Close()
	for i := range 30 {
		month := 9 + i/10
		a.Write(Entry{Time: time.Date(2026, time.Month(month), 1+i%10, 0, 0, 0, 0, time.UTC), Actor: "x", Procedure: "p", Subject: fmt.Sprint(i), Outcome: OutcomeOK})
	}
	tests := []struct {
		n         int
		wantFirst int
		wantLen   int
	}{
		{0, 0, 0}, {-1, 0, 0}, {1, 29, 1}, {3, 27, 3}, {10, 20, 10}, {11, 19, 11}, {25, 5, 25}, {30, 0, 30}, {100, 0, 30},
	}
	for _, tt := range tests {
		t.Run(fmt.Sprint(tt.n), func(t *testing.T) {
			got, err := Tail(dir, tt.n)
			if err != nil || len(got) != tt.wantLen {
				t.Fatalf("len = %d, err = %v, want %d", len(got), err, tt.wantLen)
			}
			for i, e := range got {
				if want := fmt.Sprint(tt.wantFirst + i); e.Subject != want {
					t.Fatalf("entry %d subject = %s, want %s", i, e.Subject, want)
				}
			}
		})
	}
}
