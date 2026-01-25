// Collections Benchmark - Go implementation
// Output format: BENCH:collections:<test>:<result>:<time_ms>
package main

import (
	"fmt"
	"time"
)

const numElements = 100000

func main() {
	// Build
	start := time.Now()
	data := make([]int64, numElements)
	for i := int64(0); i < numElements; i++ {
		data[i] = i
	}
	elapsed := time.Since(start).Milliseconds()
	fmt.Printf("BENCH:collections:build-100k:%d:%d\n", len(data), elapsed)

	// Map (double each)
	start = time.Now()
	mapped := make([]int64, len(data))
	for i, v := range data {
		mapped[i] = v * 2
	}
	elapsed = time.Since(start).Milliseconds()
	fmt.Printf("BENCH:collections:map-double:%d:%d\n", len(mapped), elapsed)

	// Filter (keep evens)
	start = time.Now()
	filtered := make([]int64, 0, len(data)/2)
	for _, v := range data {
		if v%2 == 0 {
			filtered = append(filtered, v)
		}
	}
	elapsed = time.Since(start).Milliseconds()
	fmt.Printf("BENCH:collections:filter-evens:%d:%d\n", len(filtered), elapsed)

	// Fold (sum)
	start = time.Now()
	var total int64 = 0
	for _, v := range data {
		total += v
	}
	elapsed = time.Since(start).Milliseconds()
	fmt.Printf("BENCH:collections:fold-sum:%d:%d\n", total, elapsed)

	// Chain (map -> filter -> fold)
	start = time.Now()
	var result int64 = 0
	for _, v := range data {
		tripled := v * 3
		if tripled%2 == 0 {
			result += tripled
		}
	}
	elapsed = time.Since(start).Milliseconds()
	fmt.Printf("BENCH:collections:chain:%d:%d\n", result, elapsed)
}
