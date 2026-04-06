#!/bin/bash

# Script to regenerate C++ SDK client from latest lib.cpp build
# This ensures we're always comparing against the most current C++ module

set -e  # Exit on any error
# Detect the correct emcmake command for cross-platform compatibility
detect_emcmake_command() {
    if command -v emcmake >/dev/null 2>&1; then
        echo "emcmake"
    elif command -v emcmake.bat >/dev/null 2>&1; then
        echo "emcmake.bat"
    elif [ -n "$EMSDK" ] && [ -f "$EMSDK/emcmake.bat" ]; then
        echo "$EMSDK/emcmake.bat"
    else
        echo "emcmake"  # fallback, will fail with clear error
    fi
}

EMCMAKE_CMD=$(detect_emcmake_command)
echo "Using emcmake command: $EMCMAKE_CMD"

CPP_DIR="cpp-sdk-test"
SDK_TEST_DIR=$(realpath "../../../../modules/sdk-test-cpp")
WASM_PATH="$SDK_TEST_DIR/build/lib.wasm"
CLI_PATH=$(realpath "../../../../target/release/spacetimedb-cli")
NUM_CORES=16

# Check if rebuild is needed
check_needs_rebuild() {
    local wasm_file="$1"
    local source_dir="$2"
    local lib_dir="../.." # Path to bindings-cpp
    
    # If WASM doesn't exist, we need to build
    if [ ! -f "$wasm_file" ]; then
        return 0  # true - needs rebuild
    fi
    
    # Find the newest source file in sdk-test-cpp
    local newest_src=$(find "$source_dir/src" "$source_dir/include" -type f \( -name "*.cpp" -o -name "*.h" \) -printf '%T@\n' 2>/dev/null | sort -n | tail -1)
    
    # Find the newest library file in bindings-cpp
    local newest_lib=$(find "$lib_dir/include" "$lib_dir/src" -type f \( -name "*.cpp" -o -name "*.h" \) -printf '%T@\n' 2>/dev/null | sort -n | tail -1)
    
    # Get WASM modification time
    local wasm_time=$(stat -c %Y "$wasm_file" 2>/dev/null || echo 0)
    
    # Check if any source is newer than WASM
    if [ -n "$newest_src" ] && [ $(echo "$newest_src" | cut -d. -f1) -gt $wasm_time ]; then
        return 0  # true - needs rebuild
    fi
    
    # Check if any library file is newer than WASM
    if [ -n "$newest_lib" ] && [ $(echo "$newest_lib" | cut -d. -f1) -gt $wasm_time ]; then
        return 0  # true - needs rebuild
    fi
    
    return 1  # false - no rebuild needed
}

echo "Regenerating C++ SDK client from latest lib.cpp build..."
echo "========================================================="

# Check if lib.cpp needs rebuilding
if check_needs_rebuild "$WASM_PATH" "$SDK_TEST_DIR"; then
    echo "Source files changed - rebuilding lib.cpp with $NUM_CORES cores..."
    echo "-----------------------------------------"
    cd "$SDK_TEST_DIR"
    
    # Check if build directory exists, if not configure it
    if [ ! -d "build" ]; then
        echo "Configuring build directory..."
        $EMCMAKE_CMD cmake -B build -DMODULE_SOURCE=src/lib.cpp -DOUTPUT_NAME=lib
    fi
    
    # Build with parallel compilation
    echo "Building lib.wasm..."
    cmake --build build -j$NUM_CORES
    
    if [ ! -f "$WASM_PATH" ]; then
        echo "ERROR: Build succeeded but lib.wasm not found at $WASM_PATH"
        echo "Build may have produced a different output file."
        exit 1
    fi
    
    echo "✅ lib.cpp built successfully!"
    echo "WASM size: $(du -h "$WASM_PATH" | cut -f1)"
    echo ""
    
    # Return to the client-comparison directory
    cd - > /dev/null
else
    echo "✅ No source changes detected - using existing lib.wasm"
    echo "WASM size: $(du -h "$WASM_PATH" | cut -f1)"
    echo ""
fi

if [ ! -f "$CLI_PATH" ]; then
    echo "ERROR: SpacetimeDB CLI not found at $CLI_PATH"
    echo "Please build the CLI first, from the project root:"
    echo "  cargo build --release -p spacetimedb-cli"
    exit 1
fi

# Clear existing C++ client directory
echo "Clearing existing C++ client directory..."
if [ -d "$CPP_DIR" ]; then
    rm -rf "$CPP_DIR"/*
    echo "Cleared $CPP_DIR/"
else
    mkdir -p "$CPP_DIR"
    echo "Created $CPP_DIR/"
fi

# Generate new C++ client
echo "Generating new C++ client from lib.wasm..."
cd "$CPP_DIR"
"$CLI_PATH" generate --lang rust --out-dir . --bin-path "$WASM_PATH" >/dev/null 2>&1

if [ $? -eq 0 ]; then
    echo ""
    echo "✅ C++ SDK client regenerated successfully!"
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