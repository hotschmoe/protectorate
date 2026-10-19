// One-shot node probe: reads a JSON request on stdin, writes a JSON reply.
// Meant to run as an sshd forced command so SSH is both transport and gate.
package main

import (
	"bufio"
	"encoding/json"
	"os"
	"runtime"
	"time"
)

type req struct {
	Method string `json:"method"`
}

type metrics struct {
	Hostname string    `json:"hostname"`
	OS       string    `json:"os"`
	Arch     string    `json:"arch"`
	NumCPU   int       `json:"num_cpu"`
	Time     time.Time `json:"time"`
}

func main() {
	enc := json.NewEncoder(os.Stdout)
	sc := bufio.NewScanner(os.Stdin)
	for sc.Scan() {
		var r req
		if err := json.Unmarshal(sc.Bytes(), &r); err != nil {
			enc.Encode(map[string]string{"error": err.Error()})
			continue
		}
		switch r.Method {
		case "metrics":
			h, _ := os.Hostname()
			enc.Encode(metrics{h, runtime.GOOS, runtime.GOARCH, runtime.NumCPU(), time.Now()})
		default:
			enc.Encode(map[string]string{"error": "method not allowed: " + r.Method})
		}
	}
}
