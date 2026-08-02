package main

import (
	"flag"
	"fmt"
	"net/http"
	"os"
)

func main() {
	health := flag.Bool("health", false, "healthcheck then exit 0")
	listen := flag.String("listen", ":8080", "listen address")
	flag.Parse()
	if *health {
		os.Exit(0)
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/", func(w http.ResponseWriter, _ *http.Request) {
		fmt.Fprintln(w, "kitchen-sink ok")
	})
	addr := *listen
	fmt.Fprintln(os.Stderr, "listening on", addr)
	if err := http.ListenAndServe(addr, mux); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
