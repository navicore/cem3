// Prime counting benchmark
// Build: go build -o primes_go primes.go

package main

import (
	"fmt"
	"os"
)

func isPrime(n int64) bool {
	if n < 2 {
		return false
	}
	if n == 2 {
		return true
	}
	for divisor := int64(2); divisor*divisor <= n; divisor++ {
		if n%divisor == 0 {
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
	result := countPrimes(100_000)
	fmt.Println(result)

	// Expected: 9592
	if result == 9592 {
		os.Exit(0)
	} else {
		os.Exit(1)
	}
}
