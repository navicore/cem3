// Skynet Benchmark - Go version
//
// Spawns 1,000,000 goroutines in a 10-ary tree structure.
// Each leaf goroutine sends its ID to its parent.
// Parent goroutines sum their 10 children's values.
// The root should return 499999500000 (sum of 0..999999).

package main

import (
	"fmt"
	"time"
)

func skynet(result chan<- int64, num, size int64) {
	if size == 1 {
		result <- num
		return
	}

	// Create channel for children
	children := make(chan int64, 10)
	childSize := size / 10

	// Spawn 10 children
	for i := int64(0); i < 10; i++ {
		go skynet(children, num+i*childSize, childSize)
	}

	// Sum results from children
	var sum int64
	for i := 0; i < 10; i++ {
		sum += <-children
	}

	result <- sum
}

func main() {
	start := time.Now()

	result := make(chan int64)
	go skynet(result, 0, 1000000)

	sum := <-result

	elapsed := time.Since(start)

	fmt.Printf("Result: %d\n", sum)
	fmt.Printf("Time: %d ms\n", elapsed.Milliseconds())
}
