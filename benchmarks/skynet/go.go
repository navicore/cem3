// Skynet Benchmark - Go implementation
// Output format: BENCH:skynet:<test>:<result>:<time_ms>
//
// Spawns goroutines in a 10-ary tree structure.
// 100,000 goroutines total.
// Expected result: sum of 0..99999 = 4999950000
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

	children := make(chan int64, 10)
	childSize := size / 10

	for i := int64(0); i < 10; i++ {
		go skynet(children, num+i*childSize, childSize)
	}

	var sum int64
	for i := 0; i < 10; i++ {
		sum += <-children
	}

	result <- sum
}

func main() {
	start := time.Now()

	result := make(chan int64)
	go skynet(result, 0, 100000)

	sum := <-result

	elapsed := time.Since(start).Milliseconds()

	fmt.Printf("BENCH:skynet:spawn-100k:%d:%d\n", sum, elapsed)
}
