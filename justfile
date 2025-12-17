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
        # Get category and name (e.g., examples/basics/hello-world.seq -> basics-hello-world)
        category=$(dirname "$file" | sed 's|examples/||' | sed 's|examples||')
        name=$(basename "$file" .seq)
        if [ -z "$category" ]; then
            output_name="$name"
        else
            output_name="${category}-${name}"
        fi
        echo "  Compiling $file..."
        target/release/seqc "$file" -o "target/examples/$output_name"
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
ci: fmt-check lint test build build-examples test-integration
    @echo ""
    @echo "✅ All CI checks passed!"
    @echo "   - Code formatting ✓"
    @echo "   - Clippy lints ✓"
    @echo "   - Unit tests ✓"
    @echo "   - Compiler built ✓"
    @echo "   - LSP server built ✓"
    @echo "   - Examples built ✓"
    @echo "   - Integration tests ✓"
    @echo ""
    @echo "Safe to push to GitHub - CI will pass."

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

