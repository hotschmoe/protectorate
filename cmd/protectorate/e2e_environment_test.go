package main

import (
	"fmt"
	"os"
	"testing"
)

func TestMain(m *testing.M) {
	for _, key := range []string{"PROTECTORATE_HUB", "PROTECTORATE_TOKEN", "PROTECTORATE_TOKEN_FILE", "PROTECTORATE_NODE_TOKEN"} {
		if err := os.Unsetenv(key); err != nil {
			fmt.Fprintf(os.Stderr, "isolate CLI fixture environment %s: %v\n", key, err)
			os.Exit(1)
		}
	}
	os.Exit(m.Run())
}
