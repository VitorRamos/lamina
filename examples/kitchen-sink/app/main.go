package main

import (
	"flag"
	"fmt"
	"net/http"
	"os"
)

func main() {
	health := flag.Bool("health", false, "healthcheck then exit")
	listen := flag.String("listen", ":8080", "listen address")
	flag.Parse()
	if *health {
		os.Exit(0)
	}
	http.HandleFunc("/", func(w http.ResponseWriter, _ *http.Request) {
		fmt.Fprintln(w, "kitchen-sink ok")
	})
	_ = http.ListenAndServe(*listen, nil)
}
