// Pingpong Benchmark - Go implementation
// Output format: BENCH:pingpong:<test>:<result>:<time_ms>
//
// Two goroutines exchange messages N times.
// Tests channel round-trip latency.
package main

import (
	"fmt"
	"time"
)

const iterations = 100000

func pong(pingChan, pongChan chan int, count int) {
	for i := 0; i < count; i++ {
		val := <-pingChan
		pongChan <- val
	}
}

func ping(pingChan, pongChan chan int, count int) {
	for i := 0; i < count; i++ {
		pingChan <- i
		<-pongChan
	}
}

func main() {
	pingChan := make(chan int)
	pongChan := make(chan int)

	start := time.Now()

	go pong(pingChan, pongChan, iterations)
	ping(pingChan, pongChan, iterations)

	elapsed := time.Since(start).Milliseconds()

	fmt.Printf("BENCH:pingpong:roundtrip-100k:%d:%d\n", iterations, elapsed)
}
