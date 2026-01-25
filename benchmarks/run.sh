#!/usr/bin/env bash
# Unified Benchmark Runner
#
# Runs all benchmarks in all languages and produces a comparison table.
#
# Usage:
#   ./run.sh             # Run all benchmarks
#   ./run.sh fibonacci   # Run only fibonacci benchmark

set -e
cd "$(dirname "$0")"

# Configuration
BENCHMARKS="fibonacci collections primes skynet pingpong fanout"
LANGUAGES="seq python go rust"
RESULTS_DIR="results"
SEQC="../target/release/seqc"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

# Parse arguments
FILTER="${1:-}"

# Setup
mkdir -p "$RESULTS_DIR"
rm -f "$RESULTS_DIR"/*.txt

echo -e "${GREEN}${BOLD}=== Seq Benchmark Suite ===${NC}"
echo

# Check dependencies
HAS_SEQ=true
HAS_PYTHON=true
HAS_GO=true
HAS_RUST=true

if [ ! -f "$SEQC" ]; then
    echo -e "${CYAN}Building seqc...${NC}"
    (cd .. && cargo build --release -p seq-compiler 2>/dev/null) || HAS_SEQ=false
fi

command -v python3 &>/dev/null || { echo -e "${YELLOW}Warning: python3 not found${NC}"; HAS_PYTHON=false; }
command -v go &>/dev/null || { echo -e "${YELLOW}Warning: go not found${NC}"; HAS_GO=false; }
command -v rustc &>/dev/null || { echo -e "${YELLOW}Warning: rustc not found${NC}"; HAS_RUST=false; }

echo

# Run a single benchmark for a single language
run_bench() {
    local bench=$1
    local lang=$2
    local output_file="$RESULTS_DIR/${bench}_${lang}.txt"

    case $lang in
        seq)
            [ "$HAS_SEQ" = false ] && { echo "SKIP:$bench:$lang:seqc not available" > "$output_file"; return; }
            local src="$bench/seq.seq"
            local bin="/tmp/bench_${bench}_seq"
            [ -f "$src" ] && "$SEQC" build "$src" -o "$bin" 2>/dev/null && "$bin" > "$output_file" 2>&1 || echo "ERROR:$bench:$lang:failed" > "$output_file"
            ;;
        python)
            [ "$HAS_PYTHON" = false ] && { echo "SKIP:$bench:$lang:python3 not available" > "$output_file"; return; }
            local src="$bench/python.py"
            [ -f "$src" ] && python3 "$src" > "$output_file" 2>&1 || echo "ERROR:$bench:$lang:failed" > "$output_file"
            ;;
        go)
            [ "$HAS_GO" = false ] && { echo "SKIP:$bench:$lang:go not available" > "$output_file"; return; }
            local src="$bench/go.go"
            local bin="/tmp/bench_${bench}_go"
            [ -f "$src" ] && go build -o "$bin" "$src" 2>/dev/null && "$bin" > "$output_file" 2>&1 || echo "ERROR:$bench:$lang:failed" > "$output_file"
            ;;
        rust)
            [ "$HAS_RUST" = false ] && { echo "SKIP:$bench:$lang:rustc not available" > "$output_file"; return; }
            local src="$bench/rust.rs"
            local bin="/tmp/bench_${bench}_rust"
            [ -f "$src" ] && rustc -O -o "$bin" "$src" 2>/dev/null && "$bin" > "$output_file" 2>&1 || echo "ERROR:$bench:$lang:failed" > "$output_file"
            ;;
    esac
}

# Run benchmarks
for bench in $BENCHMARKS; do
    [ -n "$FILTER" ] && [ "$bench" != "$FILTER" ] && continue

    echo -e "${CYAN}Running $bench benchmark...${NC}"
    for lang in $LANGUAGES; do
        printf "  %-8s " "$lang"
        run_bench "$bench" "$lang"
        if grep -q "^BENCH:" "$RESULTS_DIR/${bench}_${lang}.txt" 2>/dev/null; then
            echo -e "${GREEN}✓${NC}"
        elif grep -q "^SKIP:" "$RESULTS_DIR/${bench}_${lang}.txt" 2>/dev/null; then
            echo -e "${YELLOW}skipped${NC}"
        else
            echo -e "${RED}✗${NC}"
        fi
    done
    echo
done

# Generate report
echo -e "${GREEN}${BOLD}=== Results ===${NC}"
echo

# Helper to get time from results
get_time() {
    local suite=$1 test=$2 lang=$3
    local file="$RESULTS_DIR/${suite}_${lang}.txt"
    [ -f "$file" ] || { echo "-"; return; }
    local time=$(grep "^BENCH:${suite}:${test}:" "$file" 2>/dev/null | cut -d: -f5)
    [ -n "$time" ] && echo "${time} ms" || echo "-"
}

# Print table
print_table() {
    local suite=$1
    shift
    local tests=("$@")

    echo -e "${BOLD}$suite${NC}"
    printf "%-25s %12s %12s %12s %12s\n" "Test" "Seq" "Python" "Go" "Rust"
    printf "%-25s %12s %12s %12s %12s\n" "------------------------" "----------" "----------" "----------" "----------"

    for test in "${tests[@]}"; do
        printf "%-25s %12s %12s %12s %12s\n" \
            "$test" \
            "$(get_time "$suite" "$test" seq)" \
            "$(get_time "$suite" "$test" python)" \
            "$(get_time "$suite" "$test" go)" \
            "$(get_time "$suite" "$test" rust)"
    done
    echo
}

print_table "fibonacci" "fib-naive-30" "fib-naive-35" "fib-fast-30" "fib-fast-50" "fib-naive-20-x1000" "fib-fast-20-x1000"
print_table "collections" "build-100k" "map-double" "filter-evens" "fold-sum" "chain"
print_table "primes" "count-10k" "count-100k"
print_table "skynet" "spawn-100k"
print_table "pingpong" "roundtrip-100k"
print_table "fanout" "throughput-100k"

echo -e "${CYAN}Note: Python concurrency uses asyncio (cooperative, single-threaded).${NC}"
echo -e "${CYAN}      Go/Seq/Rust use lightweight threads or OS threads.${NC}"
