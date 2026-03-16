#!/bin/bash

# Combined script to regenerate C++ SDK client and run comparison
# This ensures we're always comparing against the most current C++ module
#
# Usage:
#   ./run_client_comparison.sh          # Smart rebuild (only if sources changed)
#   ./run_client_comparison.sh --force  # Force rebuild even if sources unchanged

set -e  # Exit on any error

# Check for force flag
FORCE_REBUILD=false
if [ "$1" = "--force" ] || [ "$1" = "-f" ]; then
    FORCE_REBUILD=true
    echo "Force rebuild requested..."
fi

echo "Regenerating SDK clients and running comparison..."
echo "=================================================="

# STEP 1a: Regenerate Rust client (with smart rebuild detection)
echo ""
echo "STEP 1a: Regenerating Rust SDK client..."
echo "========================================"

if [ ! -f scripts/regenerate_rust_client.sh ]; then
    echo "ERROR: scripts/regenerate_rust_client.sh not found!"
    echo "Make sure you're running this from the client-comparison directory."
    exit 1
fi

# Pass force flag to regenerate script if needed
if [ "$FORCE_REBUILD" = true ]; then
    # Remove existing Rust client to force rebuild
    RUST_DIR="rust-sdk-test"
    if [ -d "$RUST_DIR" ]; then
        echo "Removing existing Rust client to force rebuild..."
        rm -rf "$RUST_DIR"
    fi
fi

./scripts/regenerate_rust_client.sh

# STEP 1b: Regenerate C++ client (with smart rebuild detection)
echo ""
echo "STEP 1b: Regenerating C++ SDK client..."
echo "======================================="

if [ ! -f scripts/regenerate_cpp_client.sh ]; then
    echo "ERROR: scripts/regenerate_cpp_client.sh not found!"
    echo "Make sure you're running this from the client-comparison directory."
    exit 1
fi

# Pass force flag to regenerate script if needed
if [ "$FORCE_REBUILD" = true ]; then
    # Temporarily remove the WASM to force rebuild
    WASM_PATH="../../../../modules/sdk-test-cpp/build/lib.wasm"
    if [ -f "$WASM_PATH" ]; then
        echo "Removing existing WASM to force rebuild..."
        rm "$WASM_PATH"
    fi
fi

./scripts/regenerate_cpp_client.sh

# STEP 2: Run comparisons in parallel
echo ""
echo "STEP 2: Running comparisons..."
echo "=============================="

# Check if comparison scripts exist
if [ ! -f scripts/compare_clients.sh ]; then
    echo "ERROR: scripts/compare_clients.sh not found!"
    echo "Make sure you're running this from the client-comparison directory."
    exit 1
fi

if [ ! -f scripts/compare_modules.sh ]; then
    echo "ERROR: scripts/compare_modules.sh not found!"
    echo "Make sure you're running this from the client-comparison directory."
    exit 1
fi

echo "Running client and module comparisons in parallel..."

# Run both comparisons in parallel
./scripts/compare_clients.sh &
CLIENT_PID=$!

./scripts/compare_modules.sh &
MODULE_PID=$!

# Wait for both to complete
echo "Waiting for client comparison to complete..."
wait $CLIENT_PID
CLIENT_EXIT=$?

echo "Waiting for module comparison to complete..."
wait $MODULE_PID  
MODULE_EXIT=$?

# Check if both succeeded
if [ $CLIENT_EXIT -ne 0 ]; then
    echo "❌ Client comparison failed!"
fi

if [ $MODULE_EXIT -ne 0 ]; then
    echo "❌ Module comparison failed!"
fi

if [ $CLIENT_EXIT -eq 0 ] && [ $MODULE_EXIT -eq 0 ]; then
    echo "✅ Both comparisons completed successfully!"
else
    echo "⚠️  One or more comparisons failed - check output above"
fi

# STEP 3: Show results summary
echo ""
echo "STEP 3: Results Summary"
echo "======================="

# Client comparison results
if [ -f client_diff_analysis.txt ]; then
    echo ""
    echo "📊 Client Comparison Summary:"
    echo "$(grep "Total files compared:" client_diff_analysis.txt)"
    echo "$(grep "Identical files (ignoring version):" client_diff_analysis.txt)"
    echo "$(grep "Different files:" client_diff_analysis.txt)"
    
    # Extract file counts
    RUST_COUNT=$(grep "Rust SDK files:" client_diff_analysis.txt | cut -d: -f2 | tr -d ' ')
    CPP_COUNT=$(grep "C++ SDK files:" client_diff_analysis.txt | cut -d: -f2 | tr -d ' ')
    
    if [ "$RUST_COUNT" = "$CPP_COUNT" ]; then
        echo "File count: ✅ Perfect match ($CPP_COUNT files each)"
    else
        echo "File count: ⚠️  Mismatch (Rust: $RUST_COUNT, C++: $CPP_COUNT)"
    fi
    
    echo ""
    echo "📁 Client analysis: client_diff_analysis.txt ($(du -h client_diff_analysis.txt | cut -f1))"
else
    echo "❌ Client analysis file not found!"
fi

# Module comparison results
if [ -f module_diff_analysis.txt ]; then
    echo ""
    echo "🔧 Module Schema Summary:"
    RUST_TYPES=$(grep "Rust SDK:" module_diff_analysis.txt | grep "named types" | cut -d: -f2 | cut -d' ' -f2)
    CPP_TYPES=$(grep "C++ SDK:" module_diff_analysis.txt | grep "named types" | cut -d: -f2 | cut -d' ' -f3)
    
    echo "Rust schema: $RUST_TYPES named types"
    echo "C++ schema: $CPP_TYPES named types"
    
    if [ "$RUST_TYPES" = "$CPP_TYPES" ]; then
        echo "Schema types: ✅ Perfect match ($CPP_TYPES types each)"
    else
        echo "Schema types: ⚠️  Mismatch (difference: $((CPP_TYPES - RUST_TYPES)))"
    fi
    
    echo ""
    echo "📁 Module analysis: module_diff_analysis.txt ($(du -h module_diff_analysis.txt | cut -f1))"
else
    echo "❌ Module analysis file not found!"
fi

echo ""
echo "🎉 Regeneration and comparison complete!"
echo ""
echo "Usage tips:"
echo "  • ./run_client_comparison.sh          # Smart rebuild (only if sources changed)"
echo "  • ./run_client_comparison.sh --force  # Force rebuild even if unchanged"
echo ""
echo "Next steps:"
echo "  • Review client_diff_analysis.txt for client generation differences"
echo "  • Review module_diff_analysis.txt for schema-level differences"
echo "  • Focus on remaining type resolution issues if any"
echo "  • Check for any new regressions or improvements"