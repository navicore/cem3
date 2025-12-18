#!/bin/bash
# Benchmark runner for Seq vs Go concurrency comparison
#
# Usage:
#   ./run.sh           # Run all benchmarks
#   ./run.sh skynet    # Run only skynet benchmark
#   ./run.sh pingpong  # Run only pingpong benchmark
#   ./run.sh fanout    # Run only fanout benchmark

set -e
cd "$(dirname "$0")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

echo -e "${GREEN}=== Seq vs Go Concurrency Benchmarks ===${NC}"
echo

# Check for Go
if ! command -v go &> /dev/null; then
    echo -e "${RED}Error: go not found. Please install Go.${NC}"
    exit 1
fi

# Check for hyperfine (use silently if available)
USE_HYPERFINE=false
command -v hyperfine &> /dev/null && USE_HYPERFINE=true

# Build seqc in release mode
echo -e "${GREEN}Building seqc (release mode)...${NC}"
(cd .. && cargo build --release -p seq-compiler 2>/dev/null)
SEQC="../target/release/seqc"

# Function to build and run a benchmark
run_benchmark() {
    local name=$1
    local dir=$2

    echo -e "\n${GREEN}=== $name Benchmark ===${NC}"

    # Build Seq version
    echo "Building $name.seq..."
    $SEQC build "$dir/$name.seq" -o "$dir/$name" 2>/dev/null

    # Build Go version
    echo "Building $name.go..."
    (cd "$dir" && go build -o "${name}_go" "$name.go")

    # Run comparison
    if [ "$USE_HYPERFINE" = true ]; then
        hyperfine --warmup 2 --min-runs 5 \
            --command-name "Seq" "$dir/$name" \
            --command-name "Go" "$dir/${name}_go"
    else
        echo "Seq:"
        time "$dir/$name"
        echo
        echo "Go:"
        time "$dir/${name}_go"
    fi
}

# Determine which benchmarks to run
BENCHMARKS="${1:-all}"

case $BENCHMARKS in
    skynet)
        run_benchmark "skynet" "skynet"
        ;;
    pingpong)
        run_benchmark "pingpong" "pingpong"
        ;;
    fanout)
        run_benchmark "fanout" "fanout"
        ;;
    all)
        run_benchmark "skynet" "skynet"
        run_benchmark "pingpong" "pingpong"
        run_benchmark "fanout" "fanout"
        ;;
    *)
        echo "Unknown benchmark: $BENCHMARKS"
        echo "Usage: $0 [skynet|pingpong|fanout|all]"
        exit 1
        ;;
esac

echo -e "\n${GREEN}=== Benchmarks Complete ===${NC}"
