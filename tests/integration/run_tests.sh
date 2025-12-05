#!/usr/bin/env bash
# Integration tests for Seq
# Compiles and runs .seq files, comparing output to expected results

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
SRC_DIR="$SCRIPT_DIR/src"
EXPECTED_DIR="$SCRIPT_DIR/expected"
TMP_DIR=$(mktemp -d)
SEQC="${PROJECT_ROOT}/target/release/seqc"

# Cleanup on exit
trap "rm -rf $TMP_DIR" EXIT

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

# Track results
PASSED=0
FAILED=0
FAILURES=""

# Test cases: source file relative to examples/
TESTS=(
    "test-if"
    "test-if-else"
    "test-nested-if"
    "test-comparison"
    "test-float"
    "test-pick"
    "test-string-int"
    "test-list-ops"
    "test-map-ops"
    "test-variant-typed"
    "test_variant_access"
    "test-closure-captures"
    "recursion/factorial"
    "recursion/fibonacci"
)

echo "Running Seq integration tests..."
echo ""

for test in "${TESTS[@]}"; do
    name=$(basename "$test")
    source_file="${SRC_DIR}/${test}.seq"
    expected_file="${EXPECTED_DIR}/${name}.txt"
    binary="${TMP_DIR}/${name}"
    actual="${TMP_DIR}/${name}.out"

    printf "  %-30s " "$name"

    # Check source exists
    if [ ! -f "$source_file" ]; then
        echo -e "${RED}SKIP${NC} (source not found)"
        continue
    fi

    # Check expected output exists
    if [ ! -f "$expected_file" ]; then
        echo -e "${RED}SKIP${NC} (no expected output)"
        continue
    fi

    # Compile
    if ! "$SEQC" "$source_file" -o "$binary" 2>"${TMP_DIR}/${name}.compile.err"; then
        echo -e "${RED}FAIL${NC} (compile error)"
        FAILED=$((FAILED + 1))
        FAILURES="${FAILURES}\n  ${name}: compile error\n$(cat "${TMP_DIR}/${name}.compile.err")"
        continue
    fi

    # Run
    if ! "$binary" > "$actual" 2>&1; then
        echo -e "${RED}FAIL${NC} (runtime error, exit code $?)"
        FAILED=$((FAILED + 1))
        FAILURES="${FAILURES}\n  ${name}: runtime error (exit code $?)"
        continue
    fi

    # Compare output
    if diff -q "$expected_file" "$actual" > /dev/null 2>&1; then
        echo -e "${GREEN}PASS${NC}"
        PASSED=$((PASSED + 1))
    else
        echo -e "${RED}FAIL${NC} (output mismatch)"
        FAILED=$((FAILED + 1))
        FAILURES="${FAILURES}\n  ${name}: output mismatch\n$(diff "$expected_file" "$actual" | head -20)"
    fi
done

echo ""
echo "========================================"
echo "Results: ${PASSED} passed, ${FAILED} failed"

if [ $FAILED -gt 0 ]; then
    echo ""
    echo "Failures:"
    echo -e "$FAILURES"
    exit 1
fi

echo "All integration tests passed!"
