// Primes Benchmark - Go implementation
// Output format: BENCH:primes:<test>:<result>:<time_ms>
package main

import (
	"fmt"
	"time"
)

func isPrime(n int64) bool {
	if n < 2 {
		return false
	}
	if n == 2 {
		return true
	}
	if n%2 == 0 {
		return false
	}
	for i := int64(3); i*i <= n; i += 2 {
		if n%i == 0 {
			return false
		}
	}
	return true
}

func countPrimes(limit int64) int64 {
	var count int64 = 0
	for n := int64(2); n <= limit; n++ {
		if isPrime(n) {
			count++
		}
	}
	return count
}

func main() {
	// count-primes-10k
	start := time.Now()
	result := countPrimes(10000)
	elapsed := time.Since(start).Milliseconds()
	fmt.Printf("BENCH:primes:count-10k:%d:%d\n", result, elapsed)

	// count-primes-100k
	start = time.Now()
	result = countPrimes(100000)
	elapsed = time.Since(start).Milliseconds()
	fmt.Printf("BENCH:primes:count-100k:%d:%d\n", result, elapsed)
}
