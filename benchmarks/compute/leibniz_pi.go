// Leibniz formula for π benchmark
// π/4 = 1 - 1/3 + 1/5 - 1/7 + 1/9 - ...
// Build: go build -o leibniz_pi_go leibniz_pi.go

package main

import (
	"fmt"
	"math"
	"os"
)

const iterations = 100_000_000

func leibnizPi(n int64) float64 {
	sum := 0.0
	sign := 1.0
	for k := int64(0); k < n; k++ {
		sum += sign / (2.0*float64(k) + 1.0)
		sign = -sign
	}
	return sum * 4.0
}

func main() {
	pi := leibnizPi(iterations)
	fmt.Printf("%.15f\n", pi)

	// Verify result is close to π
	expected := math.Pi
	error := math.Abs(pi - expected)

	if error < 1e-7 {
		os.Exit(0)
	} else {
		fmt.Fprintf(os.Stderr, "Error too large: %e\n", error)
		os.Exit(1)
	}
}
