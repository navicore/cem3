// Fanout Benchmark - Go implementation
// Output format: BENCH:fanout:<test>:<result>:<time_ms>
//
// 1 producer, N consumer workers.
// Tests channel throughput with multiple receivers.
package main

import (
	"fmt"
	"runtime"
	"time"
)

const numMessages = 100000
const numWorkers = 10

func worker(workChan <-chan int, doneChan chan<- int) {
	count := 0
	for val := range workChan {
		if val < 0 {
			doneChan <- count
			return
		}
		count++
		runtime.Gosched()
	}
}

func main() {
	workChan := make(chan int, 100)
	doneChan := make(chan int, numWorkers)

	// Spawn workers
	for i := 0; i < numWorkers; i++ {
		go worker(workChan, doneChan)
	}

	start := time.Now()

	// Produce messages
	for i := 0; i < numMessages; i++ {
		workChan <- i
	}

	// Send sentinels
	for i := 0; i < numWorkers; i++ {
		workChan <- -1
	}

	// Collect results
	total := 0
	for i := 0; i < numWorkers; i++ {
		total += <-doneChan
	}

	elapsed := time.Since(start).Milliseconds()

	fmt.Printf("BENCH:fanout:throughput-100k:%d:%d\n", total, elapsed)
}
