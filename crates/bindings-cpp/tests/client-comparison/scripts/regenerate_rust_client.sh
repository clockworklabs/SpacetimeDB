#!/bin/bash

# Script to regenerate Rust SDK client from latest sdk-test module
# This ensures we're always comparing against the most current Rust module

set -e  # Exit on any error

RUST_DIR="rust-sdk-test"
SDK_TEST_DIR=$(realpath "../../../../modules/sdk-test")
CLI_PATH=$(realpath "../../../../target/release/spacetimedb-cli")

# Check if rebuild is needed
check_needs_rebuild() {
    local source_dir="$1"
    local target_dir="$2"
    
    # If target directory doesn't exist or is empty, we need to build
    if [ ! -d "$target_dir" ] || [ -z "$(ls -A "$target_dir" 2>/dev/null)" ]; then
        return 0  # true - needs rebuild
    fi
    
    # Find the newest source file in sdk-test
    local newest_src=$(find "$source_dir/src" -type f \( -name "*.rs" -o -name "*.toml" \) -printf '%T@\n' 2>/dev/null | sort -n | tail -1)
    
    # Find the oldest generated file in rust-sdk-test
    local oldest_gen=$(find "$target_dir" -type f -name "*.rs" -printf '%T@\n' 2>/dev/null | sort -n | head -1)
    
    # Check if any source is newer than generated files
    if [ -n "$newest_src" ] && [ -n "$oldest_gen" ] && [ $(echo "$newest_src" | cut -d. -f1) -gt $(echo "$oldest_gen" | cut -d. -f1) ]; then
        return 0  # true - needs rebuild
    fi
    
    return 1  # false - no rebuild needed
}

echo "Regenerating Rust SDK client from latest sdk-test module..."
echo "=========================================================="

# Check if sdk-test needs rebuilding
if check_needs_rebuild "$SDK_TEST_DIR" "$RUST_DIR"; then
    echo "Source files changed - regenerating Rust client..."
    echo "--------------------------------------------------"
else
    echo "✅ No source changes detected - using existing Rust client"
    if [ -d "$RUST_DIR" ]; then
        echo "Files present: $(find "$RUST_DIR" -name "*.rs" | wc -l)"
    fi
    echo ""
    exit 0
fi

if [ ! -f "$CLI_PATH" ]; then
    echo "ERROR: SpacetimeDB CLI not found at $CLI_PATH"
    echo "Please build the CLI first, from the project root:"
    echo "  cargo build --release -p spacetimedb-cli"
    exit 1
fi

if [ ! -d "$SDK_TEST_DIR" ]; then
    echo "ERROR: SDK test module not found at $SDK_TEST_DIR"
    echo "Please ensure the sdk-test module exists."
    exit 1
fi

# Clear existing Rust client directory
echo "Clearing existing Rust client directory..."
if [ -d "$RUST_DIR" ]; then
    rm -rf "$RUST_DIR"/*
    echo "Cleared $RUST_DIR/"
else
    mkdir -p "$RUST_DIR"
    echo "Created $RUST_DIR/"
fi

# Generate new Rust client
echo "Generating new Rust client from sdk-test module..."
cd "$RUST_DIR"
"$CLI_PATH" generate --lang rust --out-dir . --module-path "$SDK_TEST_DIR" >/dev/null 2>&1

if [ $? -eq 0 ]; then
    echo ""
    echo "✅ Rust SDK client regenerated successfully!"
    echo "Files generated: $(find . -name "*.rs" | wc -l)"
    echo ""
    echo "Next steps:"
    echo "  1. Run ./compare_clients.sh to generate new comparison"
    echo "  2. Review client_diff_analysis.txt for updated differences"
else
    echo ""
    echo "❌ Client generation failed!"
    echo "Check the error messages above for details."
    exit 1
fi