// Sum of squares benchmark
// Build: go build -o sum_squares_go sum_squares.go

package main

import (
	"fmt"
	"os"
)

func sumSquares(n int64) int64 {
	var acc int64 = 0
	for i := int64(1); i <= n; i++ {
		acc += i * i
	}
	return acc
}

func main() {
	result := sumSquares(1_000_000)
	fmt.Println(result)

	// Expected: 333333833333500000
	if result == 333333833333500000 {
		os.Exit(0)
	} else {
		os.Exit(1)
	}
}
