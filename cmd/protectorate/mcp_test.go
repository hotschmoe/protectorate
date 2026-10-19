package main

import "testing"

func TestCheckMCPListen(t *testing.T) {
	tests := []struct {
		addr string
		ok   bool
	}{
		{"127.0.0.1:7471", true},
		{"[::1]:7471", true},
		{"localhost:7471", true},
		{"100.64.0.7:7471", true},
		{"[fd7a:115c:a1e0::7]:7471", true},
		{":7471", false},
		{"0.0.0.0:7471", false},
		{"192.168.1.5:7471", false},
		{"example.com:7471", false},
		{"100.128.0.1:7471", false},
		{"nonsense", false},
	}
	for _, tt := range tests {
		if err := checkMCPListen(tt.addr); (err == nil) != tt.ok {
			t.Errorf("checkMCPListen(%q) = %v, want ok=%v", tt.addr, err, tt.ok)
		}
	}
}
