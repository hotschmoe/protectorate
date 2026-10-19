package main

import (
	"encoding/json"
	"log"
	"net/http"
	"os"
	"runtime"
)

func main() {
	http.HandleFunc("/metrics", func(w http.ResponseWriter, r *http.Request) {
		h, _ := os.Hostname()
		json.NewEncoder(w).Encode(map[string]any{"hostname": h, "os": runtime.GOOS, "arch": runtime.GOARCH, "numcpu": runtime.NumCPU()})
	})
	log.Fatal(http.ListenAndServe(":8081", nil))
}
