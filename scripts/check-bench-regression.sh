#!/bin/bash
# Check for benchmark regressions against baseline
# Fails if any Seq benchmark regresses more than THRESHOLD percent

set -euo pipefail

THRESHOLD=20  # Percent regression that triggers failure
BASELINE_DIR="benchmarks/baseline"
RESULTS_DIR="benchmarks/results"
REPORT_FILE="benchmarks/regression-report.txt"

# Clear previous report
> "$REPORT_FILE"

echo "Checking for benchmark regressions (threshold: ${THRESHOLD}%)..."
echo ""

regression_found=0

# Only check Seq results (we care about our own performance)
for result_file in "$RESULTS_DIR"/*_seq.txt; do
    if [ ! -f "$result_file" ]; then
        continue
    fi

    filename=$(basename "$result_file")
    baseline_file="$BASELINE_DIR/$filename"

    if [ ! -f "$baseline_file" ]; then
        echo "⚠️  No baseline for $filename (skipping)"
        continue
    fi

    echo "Checking $filename..."

    # Compare each benchmark line
    while IFS= read -r line; do
        # Format: BENCH:category:test:result:time_ms
        # Extract test name and time
        test_name=$(echo "$line" | cut -d: -f2-3)
        current_time=$(echo "$line" | rev | cut -d: -f1 | rev)

        # Skip if time is not a number (malformed line)
        if ! [[ "$current_time" =~ ^[0-9]+$ ]]; then
            continue
        fi

        # Find matching baseline
        baseline_line=$(grep "^BENCH:$test_name:" "$baseline_file" 2>/dev/null || true)
        if [ -z "$baseline_line" ]; then
            continue
        fi

        baseline_time=$(echo "$baseline_line" | rev | cut -d: -f1 | rev)

        # Skip if baseline time is 0 (can't compute percentage)
        if [ "$baseline_time" -eq 0 ]; then
            continue
        fi

        # Calculate regression percentage
        # (current - baseline) / baseline * 100
        diff=$((current_time - baseline_time))
        pct=$((diff * 100 / baseline_time))

        if [ "$pct" -gt "$THRESHOLD" ]; then
            echo "  🔴 REGRESSION: $test_name"
            echo "     Baseline: ${baseline_time}ms → Current: ${current_time}ms (+${pct}%)"
            echo "$test_name: ${baseline_time}ms → ${current_time}ms (+${pct}%)" >> "$REPORT_FILE"
            regression_found=1
        elif [ "$pct" -lt "-$THRESHOLD" ]; then
            echo "  🟢 IMPROVEMENT: $test_name"
            echo "     Baseline: ${baseline_time}ms → Current: ${current_time}ms (${pct}%)"
        fi
    done < "$result_file"
done

echo ""

if [ "$regression_found" -eq 1 ]; then
    echo "❌ Benchmark regressions detected!"
    echo ""
    echo "Regression report:"
    cat "$REPORT_FILE"
    exit 1
else
    echo "✅ No significant regressions detected"
    exit 0
fi
