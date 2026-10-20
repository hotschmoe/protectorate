package auth

import (
	"reflect"
	"sort"
	"strings"
	"testing"
)

func FuzzScopeNormalization(f *testing.F) {
	for _, seed := range [][2]string{{"read", "read"}, {"job_*", "events"}, {"node_admin:*", "read"}, {"[", "terminal"}, {"sleeve_*", "chat_*"}, {"read\x00", "*"}} {
		f.Add(seed[0], seed[1])
	}
	f.Fuzz(func(t *testing.T, first, second string) {
		input := []string{first, second, first}
		normalized, err := ValidateScopes(input)
		if err != nil {
			return
		}
		if !sort.StringsAreSorted(normalized) {
			t.Fatalf("noncanonical scope order: %q", normalized)
		}
		seen := map[string]bool{}
		for _, scope := range normalized {
			if seen[scope] || strings.HasPrefix(scope, ScopeNodeAdminPrefix) || (scope != first && scope != second) {
				t.Fatalf("normalization duplicated or widened authority: input=%q output=%q", input, normalized)
			}
			seen[scope] = true
		}
		if !seen[first] || !seen[second] {
			t.Fatalf("normalization silently discarded a requested scope: input=%q output=%q", input, normalized)
		}
		again, err := ValidateScopes(normalized)
		if err != nil || !reflect.DeepEqual(again, normalized) {
			t.Fatalf("normalization is not stable: first=%q second=%q err=%v", normalized, again, err)
		}
	})
}
