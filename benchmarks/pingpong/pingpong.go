// Ping-Pong Benchmark - Go version
//
// Two goroutines exchange messages N times.
// Tests: channel round-trip latency, context switch overhead.

package main

import (
	"fmt"
	"time"
)

const iterations = 1000000

func pong(pingChan <-chan int, pongChan chan<- int) {
	for i := 0; i < iterations; i++ {
		val := <-pingChan
		pongChan <- val
	}
}

func main() {
	pingChan := make(chan int)
	pongChan := make(chan int)

	start := time.Now()

	// Start pong goroutine
	go pong(pingChan, pongChan)

	// Ping in main goroutine
	for i := 0; i < iterations; i++ {
		pingChan <- i
		<-pongChan
	}

	elapsed := time.Since(start)

	fmt.Printf("%d round trips in %d ms\n", iterations, elapsed.Milliseconds())

	// Calculate throughput
	totalMessages := iterations * 2
	msgsPerSec := int64(totalMessages) * 1000 / elapsed.Milliseconds()
	fmt.Printf("Throughput: %d msg/sec\n", msgsPerSec)
}
