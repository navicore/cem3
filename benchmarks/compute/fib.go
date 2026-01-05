// Fibonacci benchmark - naive recursive implementation
// Build: go build -o fib_go fib.go

package main

import (
	"fmt"
	"os"
)

func fib(n int64) int64 {
	if n < 2 {
		return n
	}
	return fib(n-1) + fib(n-2)
}

func main() {
	result := fib(40)
	fmt.Println(result)

	// Expected: 102334155
	if result == 102334155 {
		os.Exit(0)
	} else {
		os.Exit(1)
	}
}
