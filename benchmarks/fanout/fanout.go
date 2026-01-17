// Fan-Out Benchmark - Go version
//
// 1 producer sends N messages to a shared channel.
// M workers compete to receive and process messages.
// Tests: channel contention, work distribution, scheduler fairness.

package main

import (
	"fmt"
	"time"
)

const (
	numMessages = 1000000
	numWorkers  = 100
)

func worker(workChan <-chan int, doneChan chan<- int) {
	count := 0
	for range workChan {
		count++
	}
	doneChan <- count
}

func main() {
	workChan := make(chan int, 1000) // Buffered for better throughput
	doneChan := make(chan int, numWorkers)

	start := time.Now()

	// Spawn workers
	for i := 0; i < numWorkers; i++ {
		go worker(workChan, doneChan)
	}

	// Producer: send messages
	for i := 0; i < numMessages; i++ {
		workChan <- i
	}
	close(workChan) // Signal workers to exit

	// Collect results
	total := 0
	for i := 0; i < numWorkers; i++ {
		total += <-doneChan
	}

	elapsed := time.Since(start)

	fmt.Printf("Processed: %d messages\n", total)
	fmt.Printf("Time: %d ms\n", elapsed.Milliseconds())
	fmt.Printf("Workers: %d\n", numWorkers)

	// Calculate throughput
	msgsPerSec := int64(numMessages) * 1000 / elapsed.Milliseconds()
	fmt.Printf("Throughput: %d msg/sec\n", msgsPerSec)
}
