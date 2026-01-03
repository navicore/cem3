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
YELLOW='\033[0;33m'
CYAN='\033[0;36m'
BOLD='\033[1m'
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
declare -a GO_TIMES
declare -a RATIOS

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

    # Run comparison and capture results
    if [ "$USE_HYPERFINE" = true ]; then
        local json_file=$(mktemp)
        hyperfine --warmup 2 --min-runs 5 \
            --command-name "Seq" "$dir/$name" \
            --command-name "Go" "$dir/${name}_go" \
            --export-json "$json_file"

        # Parse JSON to extract mean times (in seconds)
        local seq_time go_time
        if [ "$USE_JQ" = true ]; then
            # Robust JSON parsing with jq
            seq_time=$(jq -r '.results[] | select(.command == "Seq") | .mean' "$json_file")
            go_time=$(jq -r '.results[] | select(.command == "Go") | .mean' "$json_file")
        else
            # Fallback to grep/sed (less robust but works without jq)
            seq_time=$(grep -A20 '"command": "Seq"' "$json_file" | grep '"mean"' | head -1 | sed 's/.*: //' | sed 's/,//')
            go_time=$(grep -A20 '"command": "Go"' "$json_file" | grep '"mean"' | head -1 | sed 's/.*: //' | sed 's/,//')
        fi
        rm -f "$json_file"

        # Store results
        BENCH_NAMES+=("$name")
        SEQ_TIMES+=("$seq_time")
        GO_TIMES+=("$go_time")

        # Calculate ratio
        if [ -n "$seq_time" ] && [ -n "$go_time" ]; then
            local ratio=$(echo "scale=2; $seq_time / $go_time" | bc)
            RATIOS+=("$ratio")
        else
            RATIOS+=("N/A")
        fi
    else
        echo "Seq:"
        time "$dir/$name"
        echo
        echo "Go:"
        time "$dir/${name}_go"

        # Store placeholder for non-hyperfine runs
        BENCH_NAMES+=("$name")
        SEQ_TIMES+=("(see above)")
        GO_TIMES+=("(see above)")
        RATIOS+=("(manual)")
    fi
}

# Print summary table
print_summary() {
    if [ ${#BENCH_NAMES[@]} -eq 0 ]; then
        return
    fi

    echo -e "\n${BOLD}${CYAN}╔═══════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BOLD}${CYAN}║                    BENCHMARK SUMMARY                      ║${NC}"
    echo -e "${BOLD}${CYAN}╠═══════════════════════════════════════════════════════════╣${NC}"
    printf "${CYAN}║${NC} ${BOLD}%-12s${NC} │ ${BOLD}%12s${NC} │ ${BOLD}%12s${NC} │ ${BOLD}%12s${NC} ${CYAN}║${NC}\n" \
        "Benchmark" "Seq" "Go" "Ratio"
    echo -e "${CYAN}╠═══════════════════════════════════════════════════════════╣${NC}"

    for i in "${!BENCH_NAMES[@]}"; do
        local name="${BENCH_NAMES[$i]}"
        local seq="${SEQ_TIMES[$i]}"
        local go="${GO_TIMES[$i]}"
        local ratio="${RATIOS[$i]}"

        # Format times as milliseconds if they're numbers
        local seq_fmt="$seq"
        local go_fmt="$go"
        if [[ "$seq" =~ ^[0-9.]+$ ]]; then
            seq_fmt=$(printf "%.0f ms" $(echo "$seq * 1000" | bc))
        fi
        if [[ "$go" =~ ^[0-9.]+$ ]]; then
            go_fmt=$(printf "%.0f ms" $(echo "$go * 1000" | bc))
        fi

        # Color the ratio based on performance
        local ratio_color="$NC"
        if [[ "$ratio" =~ ^[0-9.]+$ ]]; then
            local ratio_int=$(echo "$ratio" | cut -d. -f1)
            # Handle ratios less than 1 (e.g., ".98") where cut returns empty
            if [ -z "$ratio_int" ]; then
                ratio_int=0
            fi
            if [ "$ratio_int" -le 2 ]; then
                ratio_color="$GREEN"
            elif [ "$ratio_int" -le 5 ]; then
                ratio_color="$YELLOW"
            else
                ratio_color="$RED"
            fi
            ratio="${ratio}x"
        fi

        printf "${CYAN}║${NC} %-12s │ %12s │ %12s │ ${ratio_color}%12s${NC} ${CYAN}║${NC}\n" \
            "$name" "$seq_fmt" "$go_fmt" "$ratio"
    done

    echo -e "${CYAN}╚═══════════════════════════════════════════════════════════╝${NC}"

    echo -e "\n${BOLD}Legend:${NC} ${GREEN}≤2x${NC} excellent │ ${YELLOW}2-5x${NC} good │ ${RED}>5x${NC} investigate"
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
