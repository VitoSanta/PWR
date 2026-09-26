package main

import (
	"flag"
	"fmt"
	"os"

	"logstat"
)

func main() {
	top := flag.Int("top", 10, "how many paths to report")
	flag.Parse()
	report, err := logstat.Analyze(os.Stdin, *top)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	out, err := report.JSON()
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
	fmt.Println(string(out))
}
