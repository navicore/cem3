#!/bin/bash
# Compute benchmarks runner
# Compares Seq, Rust, and Go performance on pure computation tasks

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}=== Compute Benchmarks ===${NC}"
echo ""

# Build seqc if needed
SEQC="../../target/release/seqc"
if [ ! -f "$SEQC" ]; then
    echo "Building seqc..."
    (cd ../.. && cargo build --release -p seq-compiler)
fi

# Check for hyperfine
if ! command -v hyperfine &> /dev/null; then
    echo "Warning: hyperfine not found. Install with: cargo install hyperfine"
    echo "Falling back to simple timing..."
    USE_HYPERFINE=false
else
    USE_HYPERFINE=true
fi

run_benchmark() {
    local name=$1
    echo -e "${GREEN}--- $name ---${NC}"

    # Build Seq version
    echo "Building Seq..."
    $SEQC build "${name}.seq" -o "${name}_seq"

    # Build Rust version
    echo "Building Rust..."
    rustc -O -o "${name}_rust" "${name}.rs"

    # Build Go version
    echo "Building Go..."
    go build -o "${name}_go" "${name}.go"

    echo ""

    if [ "$USE_HYPERFINE" = true ]; then
        hyperfine --warmup 1 \
            -n "Seq" "./${name}_seq" \
            -n "Rust" "./${name}_rust" \
            -n "Go" "./${name}_go"
    else
        echo "Seq:"
        time "./${name}_seq"
        echo ""
        echo "Rust:"
        time "./${name}_rust"
        echo ""
        echo "Go:"
        time "./${name}_go"
    fi

    echo ""
}

# Run each benchmark
run_benchmark "fib"
run_benchmark "sum_squares"
run_benchmark "primes"

# Cleanup binaries (use wildcards to catch any benchmark binaries)
echo "Cleaning up..."
rm -f *_seq *_rust *_go

echo -e "${GREEN}Done!${NC}"
