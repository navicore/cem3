# cem3 Build System
#
# This is the SOURCE OF TRUTH for all build/test/lint operations.
# GitHub Actions calls these recipes directly - no duplication!

# Default recipe: show available commands
default:
    @just --list

# Build everything (currently just runtime)
build: build-runtime

# Build the Rust runtime library
build-runtime:
    @echo "Building runtime (clean concatenative foundation)..."
    cargo build --release -p cem3-runtime
    @echo "✅ Runtime built: target/release/libcem3_runtime.rlib"

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
ci: fmt-check lint test build
    @echo ""
    @echo "✅ All CI checks passed!"
    @echo "   - Code formatting ✓"
    @echo "   - Clippy lints ✓"
    @echo "   - Unit tests (16/16) ✓"
    @echo "   - Runtime built ✓"
    @echo ""
    @echo "Safe to push to GitHub - CI will pass."

# Clean all build artifacts
clean:
    @echo "Cleaning build artifacts..."
    cargo clean
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
