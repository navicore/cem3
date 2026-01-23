# Seq Build System
#
# This is the SOURCE OF TRUTH for all build/test/lint operations.
# GitHub Actions calls these recipes directly - no duplication!

# Default recipe: show available commands
default:
    @just --list

# Build everything (compiler + runtime + lsp)
build: build-runtime build-compiler build-lsp

install:
    @echo "Installing the compiler..."
    cargo install --path crates/compiler
    @echo "Installing the lsp server..."
    cargo install --path crates/lsp
    @echo "Installing the repl..."
    cargo install --path crates/repl

# Build the Rust runtime as static library
build-runtime:
    @echo "Building runtime (clean concatenative foundation)..."
    cargo build --release -p seq-runtime
    @echo "✅ Runtime built: target/release/libseq_runtime.a"

# Build the compiler
build-compiler:
    @echo "Building compiler..."
    cargo build --release -p seq-compiler
    @echo "✅ Compiler built: target/release/seqc"

# Build the LSP server
build-lsp:
    @echo "Building LSP server..."
    cargo build --release -p seq-lsp
    @echo "✅ LSP server built: target/release/seq-lsp"

# Build all example programs
build-examples: build
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Building examples..."
    mkdir -p target/examples
    # Find all .seq files in examples subdirectories
    find examples -name "*.seq" -type f | while read -r file; do
        # Skip library files (those without a main word definition)
        if ! grep -q '^: main\b' "$file"; then
            echo "  Skipping $file (library file, no main)"
            continue
        fi
        # Skip examples in directories with their own .toml manifest
        # These require --ffi-manifest and special dependencies (e.g., GPL readline)
        dir=$(dirname "$file")
        if ls "$dir"/*.toml >/dev/null 2>&1; then
            echo "  Skipping $file (requires external manifest, see $dir/README.md)"
            continue
        fi
        # Get category and name (e.g., examples/projects/lisp/test.seq -> projects-lisp-test)
        category=$(dirname "$file" | sed 's|examples/||' | sed 's|examples||' | sed 's|/|-|g')
        name=$(basename "$file" .seq)
        if [ -z "$category" ]; then
            output_name="$name"
        else
            output_name="${category}-${name}"
        fi
        echo "  Compiling $file..."
        target/release/seqc build "$file" -o "target/examples/$output_name"
    done
    echo "✅ Examples built in target/examples/"
    ls -lh target/examples/

# Run all Rust unit tests
test:
    @echo "Running Rust unit tests..."
    cargo test --workspace --all-targets

# Run clippy on all workspace members
lint:
    @echo "Running clippy..."
    cargo clippy --workspace --all-targets -- -D warnings

# Format all code
fmt:
    @echo "Formatting code..."
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    @echo "Checking code formatting..."
    cargo fmt --all -- --check

# Run all CI checks (same as GitHub Actions!)
# This is what developers should run before pushing
ci: fmt-check lint test build build-examples test-integration lint-seq check-bench-freshness
    @echo ""
    @echo "✅ All CI checks passed!"
    @echo "   - Code formatting ✓"
    @echo "   - Clippy lints ✓"
    @echo "   - Unit tests ✓"
    @echo "   - Compiler built ✓"
    @echo "   - LSP server built ✓"
    @echo "   - Examples built ✓"
    @echo "   - Integration tests ✓"
    @echo "   - Seq lint ✓"
    @echo "   - Benchmarks fresh ✓"
    @echo ""
    @echo "Safe to push to GitHub - CI will pass."

# Check that benchmarks have been run recently (within 24 hours)
# This catches performance regressions by ensuring benchmarks are run regularly
check-bench-freshness:
    #!/usr/bin/env bash
    set -euo pipefail
    BENCH_FILE="benchmarks/LATEST_RUN.txt"
    if [ ! -f "$BENCH_FILE" ]; then
        echo "❌ Benchmarks have never been run!"
        echo "   Run 'just bench' to establish a baseline."
        exit 1
    fi
    # Extract timestamp from file
    TIMESTAMP=$(grep "^timestamp:" "$BENCH_FILE" | cut -d' ' -f2)
    if [ -z "$TIMESTAMP" ]; then
        echo "❌ Invalid benchmark file format"
        exit 1
    fi
    # Convert to epoch seconds (works on both macOS and Linux)
    # Note: Timestamps are stored in UTC, so we must compare against UTC
    if date --version >/dev/null 2>&1; then
        # GNU date (Linux) - understands ISO 8601 with Z suffix
        BENCH_EPOCH=$(date -d "$TIMESTAMP" +%s)
    else
        # BSD date (macOS) - parse as UTC by setting TZ
        BENCH_EPOCH=$(TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%SZ" "$TIMESTAMP" +%s 2>/dev/null || TZ=UTC date -j -f "%Y-%m-%dT%H:%M:%S" "${TIMESTAMP%Z}" +%s)
    fi
    NOW_EPOCH=$(date -u +%s)
    AGE_HOURS=$(( (NOW_EPOCH - BENCH_EPOCH) / 3600 ))
    if [ $AGE_HOURS -ge 24 ]; then
        echo "❌ Benchmarks are stale ($AGE_HOURS hours old)"
        echo "   Last run: $TIMESTAMP"
        echo "   Run 'just bench' to update."
        exit 1
    fi
    echo "✅ Benchmarks are fresh ($AGE_HOURS hours old)"

# Lint all Seq source files (strict mode for CI - warnings are errors)
lint-seq: build
    @echo "Linting Seq files..."
    ./target/release/seqc lint --deny-warnings examples/ tests/ crates/compiler/stdlib/
    @echo "✅ Seq lint passed!"

# Install seq-lsp to ~/.local/bin (for neovim integration)
install-lsp: build-lsp
    @echo "Installing seq-lsp to ~/.local/bin..."
    mkdir -p ~/.local/bin
    cp target/release/seq-lsp ~/.local/bin/
    @echo "✅ seq-lsp installed to ~/.local/bin/seq-lsp"
    @echo "   Make sure ~/.local/bin is in your PATH"

# Clean all build artifacts
clean:
    @echo "Cleaning build artifacts..."
    cargo clean
    rm -f examples/*.ll
    rm -rf target/examples
    @echo "✅ Clean complete"

# Development: quick format + build + test
dev: fmt build test

# Show test output (verbose)
test-verbose:
    cargo test --workspace -- --nocapture

# Check for outdated dependencies
outdated:
    cargo outdated --workspace

# ============================================================================
# Static Analysis Tools (beyond clippy)
# Install with: just install-analysis-tools
# ============================================================================

# Install all static analysis tools
install-analysis-tools:
    @echo "Installing static analysis tools..."
    cargo install cargo-deny
    cargo install cargo-audit
    cargo install cargo-geiger
    cargo install cargo-machete
    @echo "Installing scc (complexity analyzer)..."
    @if command -v brew >/dev/null 2>&1; then \
        brew install scc; \
    elif command -v go >/dev/null 2>&1; then \
        go install github.com/boyter/scc/v3@latest; \
    else \
        echo "Please install scc manually: https://github.com/boyter/scc#install"; \
    fi
    npm install -g jscpd
    @echo "✅ All analysis tools installed"

# Run all static analysis tools
analyze: analyze-complexity analyze-duplicates analyze-deps analyze-security analyze-unsafe analyze-unused
    @echo ""
    @echo "✅ All static analysis complete! Reports saved to target/analysis/"

# Analyze code complexity (files sorted by cyclomatic complexity)
analyze-complexity:
    @mkdir -p target/analysis
    @echo "=== Code Complexity ==="
    NO_COLOR=1 scc crates/ --by-file --no-cocomo -s complexity | tee target/analysis/complexity.txt

# Detect duplicate/copy-pasted code
analyze-duplicates:
    @mkdir -p target/analysis
    @echo "=== Duplicate Code Detection ==="
    NO_COLOR=1 jscpd ./crates --ignore "**/target/**" --min-tokens 50 --min-lines 5 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | tee target/analysis/duplicates.txt

# Analyze dependencies (licenses, duplicates, advisories)
analyze-deps:
    @mkdir -p target/analysis
    @echo "=== Dependency Analysis ==="
    cargo deny --color never check 2>&1 | tee target/analysis/deps.txt || true

# Initialize cargo-deny configuration (run once)
init-deny:
    cargo deny init

# Check for security vulnerabilities in dependencies
analyze-security:
    @mkdir -p target/analysis
    @echo "=== Security Vulnerabilities ==="
    cargo audit --color never 2>&1 | sed 's/\x1b\[[0-9;]*m//g' | tee target/analysis/security.txt

# Audit unsafe code usage
analyze-unsafe:
    @mkdir -p target/analysis
    @echo "=== Unsafe Code Audit ==="
    NO_COLOR=1 cargo geiger -p seq-compiler -p seq-runtime -p seq-core -p seq-repl -p seq-lsp 2>&1 | tee target/analysis/unsafe.txt

# Find unused dependencies
analyze-unused:
    @mkdir -p target/analysis
    @echo "=== Unused Dependencies ==="
    NO_COLOR=1 cargo machete 2>&1 | tee target/analysis/unused.txt

# Generate documentation
doc:
    cargo doc --workspace --no-deps --open

# Verify workspace consistency
verify-workspace:
    @echo "Verifying workspace configuration..."
    cargo tree --workspace
    @echo "✅ Workspace verified"

# Run the critical tests that validate Seq's design
test-critical:
    @echo "Running critical design validation tests..."
    cargo test test_critical_shuffle_pattern
    cargo test test_multifield_variant_survives_shuffle
    @echo "✅ Core design validated!"

# Run integration tests (compile and run .seq programs)
test-integration: build
    @echo "Running integration tests..."
    ./target/release/seqc test tests/integration/src/
    @echo "✅ Integration tests passed!"

# Run all benchmarks (Seq vs Go comparison)
bench: build
    @echo "Running all benchmarks..."
    cd benchmarks && ./run.sh

# Run skynet benchmark (spawn overhead - 1M strands)
bench-skynet: build
    @echo "Running skynet benchmark..."
    cd benchmarks && ./run.sh skynet

# Run pingpong benchmark (channel latency - 1M messages)
bench-pingpong: build
    @echo "Running pingpong benchmark..."
    cd benchmarks && ./run.sh pingpong

# Run fanout benchmark (channel throughput - 100 workers, 1M messages)
bench-fanout: build
    @echo "Running fanout benchmark..."
    cd benchmarks && ./run.sh fanout

