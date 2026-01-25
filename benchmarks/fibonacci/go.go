// Fibonacci Benchmark - Go implementation
// Output format: BENCH:fibonacci:<test>:<result>:<time_ms>
package main

import (
	"fmt"
	"time"
)

func fibNaive(n int64) int64 {
	if n < 2 {
		return n
	}
	return fibNaive(n-1) + fibNaive(n-2)
}

func fibFast(n int64) int64 {
	if n < 2 {
		return n
	}
	a, b := int64(0), int64(1)
	for i := int64(1); i < n; i++ {
		a, b = b, a+b
	}
	return b
}

func bench(name string, n int64, expected int64, f func(int64) int64) {
	start := time.Now()
	result := f(n)
	elapsed := time.Since(start).Milliseconds()
	fmt.Printf("BENCH:fibonacci:%s:%d:%d\n", name, result, elapsed)
	if result != expected {
		fmt.Printf("ERROR: expected %d, got %d\n", expected, result)
	}
}

func benchRepeated(name string, n int64, iterations int, expected int64, f func(int64) int64) {
	start := time.Now()
	var result int64
	for i := 0; i < iterations; i++ {
		result = f(n)
	}
	elapsed := time.Since(start).Milliseconds()
	fmt.Printf("BENCH:fibonacci:%s:%d:%d\n", name, result, elapsed)
	if result != expected {
		fmt.Printf("ERROR: expected %d, got %d\n", expected, result)
	}
}

func main() {
	// Naive recursive tests
	bench("fib-naive-30", 30, 832040, fibNaive)
	bench("fib-naive-35", 35, 9227465, fibNaive)

	// Iterative tests
	bench("fib-fast-30", 30, 832040, fibFast)
	bench("fib-fast-50", 50, 12586269025, fibFast)
	bench("fib-fast-70", 70, 190392490709135, fibFast)

	// Repeated runs
	benchRepeated("fib-naive-20-x1000", 20, 1000, 6765, fibNaive)
	benchRepeated("fib-fast-20-x1000", 20, 1000, 6765, fibFast)
}
