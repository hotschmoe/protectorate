package hub

import "testing"

func TestServedLocally(t *testing.T) {
	nodes := []string{"gpu1", "llm.example.com", "203.0.113.9", "llm.hotschmoe.com"}
	tests := []struct {
		url  string
		want bool
	}{
		{"", false},
		{"https://api.deepseek.com/v1", false},
		{"https://api.anthropic.com", false},
		{"https://openrouter.ai/api/v1", false},
		{"https://1.1.1.1/v1", false},
		{"http://localhost:11434/v1", true},
		{"http://127.0.0.1:8000/v1", true},
		{"http://[::1]:8000/v1", true},
		{"http://10.1.2.3:8000", true},
		{"http://172.16.0.5", true},
		{"http://172.32.0.5", false},
		{"http://192.168.1.20:1234/v1", true},
		{"http://169.254.1.1", true},
		{"http://100.64.0.1:8000", true},
		{"http://100.127.255.254", true},
		{"http://100.128.0.1", false},
		{"http://[fd7a:115c:a1e0::1]:8000", true},
		{"http://[2001:db8::1]:8000", false},
		{"http://box.local:8000", true},
		{"http://gpu.tail1234.ts.net:8000/v1", true},
		{"http://gpu1:8000", true},
		{"http://workstation/v1", true},
		{"https://LLM.example.com/v1", true},
		{"http://203.0.113.9:8000", true},
		{"http://203.0.113.10:8000", false},
		{"https://llm.hotschmoe.com/v1", true},
		{"https://api.hotschmoe.com/v1", false},
		{"http://192.168.10.5:18080/v1", true},
		{"not a url", false},
		{"://bad", false},
	}
	for _, tc := range tests {
		if got := servedLocally(tc.url, nodes); got != tc.want {
			t.Errorf("servedLocally(%q) = %v, want %v", tc.url, got, tc.want)
		}
	}
}
