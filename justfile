# cem3 Build System
#
# This is the SOURCE OF TRUTH for all build/test/lint operations.
# GitHub Actions calls these recipes directly - no duplication!

# Default recipe: show available commands
default:
    @just --list

# Build everything (compiler + runtime)
build: build-runtime build-compiler

# Build the Rust runtime as static library
build-runtime:
    @echo "Building runtime (clean concatenative foundation)..."
    cargo build --release -p cem3-runtime
    @echo "✅ Runtime built: target/release/libcem3_runtime.a"

# Build the compiler
build-compiler:
    @echo "Building compiler..."
    cargo build --release -p cem3-compiler
    @echo "✅ Compiler built: target/release/cem3"

# Build all example programs
build-examples: build
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Building examples..."
    mkdir -p target/examples
    # Find all .cem files in examples subdirectories
    find examples -name "*.cem" -type f | while read -r file; do
        # Get category and name (e.g., examples/basics/hello-world.cem -> basics-hello-world)
        category=$(dirname "$file" | sed 's|examples/||' | sed 's|examples||')
        name=$(basename "$file" .cem)
        if [ -z "$category" ]; then
            output_name="$name"
        else
            output_name="${category}-${name}"
        fi
        echo "  Compiling $file..."
        target/release/cem3 "$file" -o "target/examples/$output_name"
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
ci: fmt-check lint test build build-examples
    @echo ""
    @echo "✅ All CI checks passed!"
    @echo "   - Code formatting ✓"
    @echo "   - Clippy lints ✓"
    @echo "   - Unit tests ✓"
    @echo "   - Compiler built ✓"
    @echo "   - Examples built ✓"
    @echo ""
    @echo "Safe to push to GitHub - CI will pass."

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

# Generate documentation
doc:
    cargo doc --workspace --no-deps --open

# Verify workspace consistency
verify-workspace:
    @echo "Verifying workspace configuration..."
    cargo tree --workspace
    @echo "✅ Workspace verified"

# Run the critical tests that validate cem3's design
test-critical:
    @echo "Running critical design validation tests..."
    cargo test test_critical_shuffle_pattern
    cargo test test_multifield_variant_survives_shuffle
    @echo "✅ Core design validated!"
