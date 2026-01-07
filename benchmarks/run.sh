#!/bin/bash
# Benchmark runner for Seq vs Go vs Rust comparison
#
# Usage:
#   ./run.sh             # Run all benchmarks
#   ./run.sh skynet      # Run only skynet benchmark
#   ./run.sh pingpong    # Run only pingpong benchmark
#   ./run.sh fanout      # Run only fanout benchmark
#   ./run.sh compute     # Run only compute benchmarks (fib, sum_squares, primes)
#   ./run.sh concurrency # Run only concurrency benchmarks

set -e
cd "$(dirname "$0")"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
NC='\033[0m'

echo -e "${GREEN}=== Seq vs Go vs Rust Benchmarks ===${NC}"
echo

# Check for Go
if ! command -v go &> /dev/null; then
    echo -e "${RED}Error: go not found. Please install Go.${NC}"
    exit 1
fi

# Check for Rust
if ! command -v rustc &> /dev/null; then
    echo -e "${RED}Error: rustc not found. Please install Rust.${NC}"
    exit 1
fi

# Check for hyperfine (use silently if available)
USE_HYPERFINE=false
command -v hyperfine &> /dev/null && USE_HYPERFINE=true

# Check for jq (optional, for robust JSON parsing)
USE_JQ=false
command -v jq &> /dev/null && USE_JQ=true

# Build seqc in release mode
echo -e "${GREEN}Building seqc (release mode)...${NC}"
(cd .. && cargo build --release -p seq-compiler 2>/dev/null)
SEQC="../target/release/seqc"

# Arrays to store results for summary
declare -a BENCH_NAMES
declare -a SEQ_TIMES
declare -a RUST_TIMES
declare -a GO_TIMES
declare -a SEQ_GO_RATIOS

# Function to build and run a concurrency benchmark (Seq vs Go vs Rust)
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

    # Build Rust version
    echo "Building $name.rs..."
    rustc -O -o "$dir/${name}_rust" "$dir/$name.rs"

    # Run comparison and capture results
    if [ "$USE_HYPERFINE" = true ]; then
        local json_file=$(mktemp)
        hyperfine --warmup 2 --min-runs 5 \
            --command-name "Seq" "$dir/$name" \
            --command-name "Rust" "$dir/${name}_rust" \
            --command-name "Go" "$dir/${name}_go" \
            --export-json "$json_file"

        # Parse JSON to extract mean times (in seconds)
        local seq_time rust_time go_time
        if [ "$USE_JQ" = true ]; then
            seq_time=$(jq -r '.results[] | select(.command == "Seq") | .mean' "$json_file")
            rust_time=$(jq -r '.results[] | select(.command == "Rust") | .mean' "$json_file")
            go_time=$(jq -r '.results[] | select(.command == "Go") | .mean' "$json_file")
        else
            seq_time=$(grep -A20 '"command": "Seq"' "$json_file" | grep '"mean"' | head -1 | sed 's/.*: //' | sed 's/,//')
            rust_time=$(grep -A20 '"command": "Rust"' "$json_file" | grep '"mean"' | head -1 | sed 's/.*: //' | sed 's/,//')
            go_time=$(grep -A20 '"command": "Go"' "$json_file" | grep '"mean"' | head -1 | sed 's/.*: //' | sed 's/,//')
        fi
        rm -f "$json_file"

        # Store results
        BENCH_NAMES+=("$name")
        SEQ_TIMES+=("$seq_time")
        RUST_TIMES+=("$rust_time")
        GO_TIMES+=("$go_time")

        # Calculate Seq/Go ratio
        if [ -n "$seq_time" ] && [ -n "$go_time" ]; then
            local ratio=$(echo "scale=2; $seq_time / $go_time" | bc)
            SEQ_GO_RATIOS+=("$ratio")
        else
            SEQ_GO_RATIOS+=("N/A")
        fi
    else
        echo "Seq:"
        time "$dir/$name"
        echo
        echo "Rust:"
        time "$dir/${name}_rust"
        echo
        echo "Go:"
        time "$dir/${name}_go"

        BENCH_NAMES+=("$name")
        SEQ_TIMES+=("(see above)")
        RUST_TIMES+=("(see above)")
        GO_TIMES+=("(see above)")
        SEQ_GO_RATIOS+=("(manual)")
    fi
}

# Function to build and run a compute benchmark (Seq vs Rust vs Go)
run_compute_benchmark() {
    local name=$1
    local dir=$2

    echo -e "\n${GREEN}=== $name (compute) ===${NC}"

    # Build all versions
    echo "Building $name.seq..."
    $SEQC build "$dir/$name.seq" -o "$dir/${name}_seq" 2>/dev/null

    echo "Building $name.rs..."
    rustc -O -o "$dir/${name}_rust" "$dir/$name.rs"

    echo "Building $name.go..."
    (cd "$dir" && go build -o "${name}_go" "$name.go")

    # Run comparison
    if [ "$USE_HYPERFINE" = true ]; then
        local json_file=$(mktemp)
        hyperfine --warmup 1 --min-runs 3 \
            --command-name "Seq" "$dir/${name}_seq" \
            --command-name "Rust" "$dir/${name}_rust" \
            --command-name "Go" "$dir/${name}_go" \
            --export-json "$json_file"

        local seq_time rust_time go_time
        if [ "$USE_JQ" = true ]; then
            seq_time=$(jq -r '.results[] | select(.command == "Seq") | .mean' "$json_file")
            rust_time=$(jq -r '.results[] | select(.command == "Rust") | .mean' "$json_file")
            go_time=$(jq -r '.results[] | select(.command == "Go") | .mean' "$json_file")
        else
            seq_time=$(grep -A20 '"command": "Seq"' "$json_file" | grep '"mean"' | head -1 | sed 's/.*: //' | sed 's/,//')
            rust_time=$(grep -A20 '"command": "Rust"' "$json_file" | grep '"mean"' | head -1 | sed 's/.*: //' | sed 's/,//')
            go_time=$(grep -A20 '"command": "Go"' "$json_file" | grep '"mean"' | head -1 | sed 's/.*: //' | sed 's/,//')
        fi
        rm -f "$json_file"

        BENCH_NAMES+=("$name")
        SEQ_TIMES+=("$seq_time")
        RUST_TIMES+=("$rust_time")
        GO_TIMES+=("$go_time")

        if [ -n "$seq_time" ] && [ -n "$go_time" ]; then
            local ratio=$(echo "scale=2; $seq_time / $go_time" | bc)
            SEQ_GO_RATIOS+=("$ratio")
        else
            SEQ_GO_RATIOS+=("N/A")
        fi
    else
        echo "Seq:"
        time "$dir/${name}_seq"
        echo
        echo "Rust:"
        time "$dir/${name}_rust"
        echo
        echo "Go:"
        time "$dir/${name}_go"

        BENCH_NAMES+=("$name")
        SEQ_TIMES+=("(see above)")
        RUST_TIMES+=("(see above)")
        GO_TIMES+=("(see above)")
        SEQ_GO_RATIOS+=("(manual)")
    fi
}

# Print summary table
print_summary() {
    if [ ${#BENCH_NAMES[@]} -eq 0 ]; then
        return
    fi

    echo -e "\n${BOLD}${CYAN}╔══════════════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}${CYAN}║                           BENCHMARK SUMMARY                              ║${NC}"
    echo -e "${BOLD}${CYAN}╠══════════════════════════════════════════════════════════════════════════╣${NC}"
    printf "${CYAN}║${NC} ${BOLD}%-12s${NC} │ ${BOLD}%12s${NC} │ ${BOLD}%12s${NC} │ ${BOLD}%12s${NC} │ ${BOLD}%12s${NC} ${CYAN}║${NC}\n" \
        "Benchmark" "Seq" "Rust" "Go" "Seq/Go"
    echo -e "${CYAN}╠══════════════════════════════════════════════════════════════════════════╣${NC}"

    for i in "${!BENCH_NAMES[@]}"; do
        local name="${BENCH_NAMES[$i]}"
        local seq="${SEQ_TIMES[$i]}"
        local rust="${RUST_TIMES[$i]}"
        local go="${GO_TIMES[$i]}"
        local ratio="${SEQ_GO_RATIOS[$i]}"

        # Format times as milliseconds if they're numbers
        local seq_fmt="$seq"
        local rust_fmt="$rust"
        local go_fmt="$go"
        if [[ "$seq" =~ ^[0-9.]+$ ]]; then
            seq_fmt=$(printf "%.0f ms" $(echo "$seq * 1000" | bc))
        fi
        if [[ "$rust" =~ ^[0-9.]+$ ]]; then
            rust_fmt=$(printf "%.0f ms" $(echo "$rust * 1000" | bc))
        fi
        if [[ "$go" =~ ^[0-9.]+$ ]]; then
            go_fmt=$(printf "%.0f ms" $(echo "$go * 1000" | bc))
        fi

        # Color the ratio based on performance
        local ratio_color="$NC"
        if [[ "$ratio" =~ ^[0-9.]+$ ]]; then
            local ratio_int=$(echo "$ratio" | cut -d. -f1)
            if [ -z "$ratio_int" ]; then
                ratio_int=0
            fi
            if [ "$ratio_int" -le 2 ]; then
                ratio_color="$GREEN"
            elif [ "$ratio_int" -le 10 ]; then
                ratio_color="$YELLOW"
            else
                ratio_color="$RED"
            fi
            ratio="${ratio}x"
        fi

        printf "${CYAN}║${NC} %-12s │ %12s │ %12s │ %12s │ ${ratio_color}%12s${NC} ${CYAN}║${NC}\n" \
            "$name" "$seq_fmt" "$rust_fmt" "$go_fmt" "$ratio"
    done

    echo -e "${CYAN}╚══════════════════════════════════════════════════════════════════════════╝${NC}"

    echo -e "\n${BOLD}Legend (Seq/Go):${NC} ${GREEN}≤2x${NC} excellent │ ${YELLOW}2-10x${NC} good │ ${RED}>10x${NC} investigate"
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
    fib)
        run_compute_benchmark "fib" "compute"
        ;;
    sum_squares)
        run_compute_benchmark "sum_squares" "compute"
        ;;
    primes)
        run_compute_benchmark "primes" "compute"
        ;;
    leibniz_pi)
        run_compute_benchmark "leibniz_pi" "compute"
        ;;
    compute)
        run_compute_benchmark "fib" "compute"
        run_compute_benchmark "sum_squares" "compute"
        run_compute_benchmark "primes" "compute"
        run_compute_benchmark "leibniz_pi" "compute"
        ;;
    concurrency)
        run_benchmark "skynet" "skynet"
        run_benchmark "pingpong" "pingpong"
        run_benchmark "fanout" "fanout"
        ;;
    all)
        echo -e "${CYAN}--- Concurrency Benchmarks ---${NC}"
        run_benchmark "skynet" "skynet"
        run_benchmark "pingpong" "pingpong"
        run_benchmark "fanout" "fanout"
        echo -e "\n${CYAN}--- Compute Benchmarks ---${NC}"
        run_compute_benchmark "fib" "compute"
        run_compute_benchmark "sum_squares" "compute"
        run_compute_benchmark "primes" "compute"
        run_compute_benchmark "leibniz_pi" "compute"
        ;;
    *)
        echo "Unknown benchmark: $BENCHMARKS"
        echo "Usage: $0 [skynet|pingpong|fanout|fib|sum_squares|primes|leibniz_pi|compute|concurrency|all]"
        exit 1
        ;;
esac

# Print consolidated summary
print_summary

echo -e "\n${GREEN}=== Benchmarks Complete ===${NC}"

# Save timestamp and commit to LATEST_RUN.txt for CI staleness check
TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
cat > LATEST_RUN.txt << EOF
# Benchmark run record - DO NOT EDIT MANUALLY
# This file is checked by CI to ensure benchmarks are run regularly
timestamp: $TIMESTAMP
commit: $COMMIT
benchmarks_run: $BENCHMARKS
EOF
echo -e "${GREEN}Updated LATEST_RUN.txt (commit: $COMMIT)${NC}"
