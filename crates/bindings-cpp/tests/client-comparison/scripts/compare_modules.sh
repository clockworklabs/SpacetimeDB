#!/bin/bash
# compare_modules.sh - Compare module schemas between Rust and C++ SDKs

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" &> /dev/null && pwd)"
PARENT_DIR="$(dirname "$SCRIPT_DIR")"
cd "$PARENT_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Detect available Python command (cross-platform)
detect_python() {
    local python_cmd=""
    
    # Try different Python commands in order of preference
    for cmd in python3 python py; do
        if command -v "$cmd" >/dev/null 2>&1; then
            # Test if the command actually works and has json module
            if "$cmd" -c "import json; import sys; print('OK')" >/dev/null 2>&1; then
                python_cmd="$cmd"
                break
            fi
        fi
    done
    
    if [ -z "$python_cmd" ]; then
        echo "❌ Error: No working Python installation found (tried: python3, python, py)" >&2
        echo "   Please install Python 3.x with json module support" >&2
        exit 1
    fi
    
    echo "$python_cmd"
}

# Set the Python command to use
PYTHON_CMD=$(detect_python)
echo "Using Python command: $PYTHON_CMD"

echo "Comparing module schemas between Rust and C++ SDKs..."
echo "=================================================="

# Function to get module schema from WASM
get_module_schema() {
    local sdk_name="$1"
    local output_file="$2"
    local wasm_path="$3"
    
    echo "Getting $sdk_name module schema from WASM..."
    
    # Use the same WASM file that client generation uses  
    local temp_db
    if [ "$sdk_name" = "Rust" ]; then
        temp_db="rust-schema-temp"
    else
        temp_db="cpp-schema-temp"
    fi
    
    echo "  Publishing WASM to temporary database: $temp_db from $wasm_path"
    if spacetime publish --bin-path "$wasm_path" "$temp_db" -c -y >/dev/null 2>&1; then
        echo "  Retrieving schema from published module..."
        if spacetime describe --json "$temp_db" > "$output_file" 2>/dev/null; then
            echo "✅ $sdk_name schema retrieved successfully from WASM"
            # Pretty print the JSON for better comparison
            $PYTHON_CMD -m json.tool "$output_file" > "${output_file}.tmp" 2>/dev/null && mv "${output_file}.tmp" "$output_file" || true
        else
            echo "❌ Failed to get $sdk_name schema from published module"
            return 1
        fi
    else
        echo "❌ Failed to publish $sdk_name WASM to temporary database"
        return 1
    fi
}

# Function to extract and analyze schema sections
analyze_schema_section() {
    local schema_file="$1"
    local section_name="$2"
    local output_prefix="$3"
    
    case "$section_name" in
        "typespace")
            # Extract first types array (line 3)
            sed -n '3,/^    ],$/p' "$schema_file" > "${output_prefix}_typespace.json"
            ;;
        "tables")
            # Extract tables section
            sed -n '/"tables":/,/^    ],$/p' "$schema_file" > "${output_prefix}_tables.json"
            ;;
        "reducers")
            # Extract reducers section
            sed -n '/"reducers":/,/^    ],$/p' "$schema_file" > "${output_prefix}_reducers.json"
            ;;
        "named_types")
            # Extract last types array (after reducers)
            awk '/"types":/ && ++count==2,/^}$/' "$schema_file" > "${output_prefix}_named_types.json"
            ;;
    esac
}

# Function to count items in a section
count_section_items() {
    local file="$1"
    local pattern="$2"
    
    if [ -f "$file" ]; then
        grep -c "$pattern" "$file" 2>/dev/null || echo "0"
    else
        echo "0"
    fi
}

# Function to get meaningful diff content only
get_meaningful_diff() {
    local file1="$1"
    local file2="$2"
    
    # Create temporary files with version comments filtered out
    local temp1=$(mktemp)
    local temp2=$(mktemp)
    
    # Filter out version-specific content and normalize spacing
    grep -v -E "(commit [a-f0-9]+|version [0-9]+\.[0-9]+\.[0-9]+)" "$file1" | sed 's/[[:space:]]*$//' > "$temp1" 2>/dev/null || true
    grep -v -E "(commit [a-f0-9]+|version [0-9]+\.[0-9]+\.[0-9]+)" "$file2" | sed 's/[[:space:]]*$//' > "$temp2" 2>/dev/null || true
    
    # Get diff content, filter to only +/- lines, remove diff headers, limit output
    diff -u "$temp1" "$temp2" 2>/dev/null | grep -E '^[+-]' | grep -v -E '^[+-]{3}' | head -50
    
    # Cleanup
    rm -f "$temp1" "$temp2"
}

# Function to find differences in named items
find_differences() {
    local rust_file="$1"
    local cpp_file="$2"
    local item_pattern="$3"
    
    # Extract names from both files
    local rust_names=$(mktemp)
    local cpp_names=$(mktemp)
    
    grep -o "$item_pattern" "$rust_file" 2>/dev/null | sort | uniq > "$rust_names" || true
    grep -o "$item_pattern" "$cpp_file" 2>/dev/null | sort | uniq > "$cpp_names" || true
    
    echo "Only in Rust:"
    comm -23 "$rust_names" "$cpp_names" | head -10 | sed 's/^/  - /'
    
    echo "Only in C++:"
    comm -13 "$rust_names" "$cpp_names" | head -10 | sed 's/^/  - /'
    
    # Cleanup
    rm -f "$rust_names" "$cpp_names"
}

# Paths - use the same WASM files that client generation uses
RUST_WASM_PATH=$(realpath "../../../../target/wasm32-unknown-unknown/release/sdk_test_module.wasm")
CPP_WASM_PATH=$(realpath "../../../../modules/sdk-test-cpp/build/lib.wasm")

RUST_SCHEMA="rust-module-schema.json"  
CPP_SCHEMA="cpp-module-schema.json"
ANALYSIS_FILE="module_diff_analysis.txt"

# Get schemas from WASM files
echo
echo "Step 1: Retrieving module schemas from WASM..."
echo "=============================================="

# Check if WASM files exist and get schemas
RUST_AVAILABLE=false
CPP_AVAILABLE=false

# Check if we have an existing Rust schema file or need to regenerate
if [ -f "$RUST_SCHEMA" ]; then
    echo "✅ Using existing Rust schema from $RUST_SCHEMA"
    RUST_AVAILABLE=true
elif [ ! -f "$RUST_WASM_PATH" ]; then
    echo "❌ Rust WASM not found at: $RUST_WASM_PATH"
    echo "   And no existing rust-module-schema.json found"
    echo "   Build it with: cargo build --target wasm32-unknown-unknown --release -p sdk-test-module"
else
    if get_module_schema "Rust" "$RUST_SCHEMA" "$RUST_WASM_PATH"; then
        RUST_AVAILABLE=true
    else
        echo "❌ Failed to get Rust module schema"
    fi
fi

# Check if we have an existing C++ schema file or need to regenerate
if [ -f "$CPP_SCHEMA" ]; then
    echo "✅ Using existing C++ schema from $CPP_SCHEMA"
    CPP_AVAILABLE=true
elif [ ! -f "$CPP_WASM_PATH" ]; then
    echo "❌ C++ WASM not found at: $CPP_WASM_PATH"
    echo "   And no existing cpp-module-schema.json found"
    echo "   Build it with: cmake --build build"
else
    if get_module_schema "C++" "$CPP_SCHEMA" "$CPP_WASM_PATH"; then
        CPP_AVAILABLE=true
    else
        echo "❌ Failed to get C++ module schema"
    fi
fi

# Exit only if neither schema is available
if [ "$RUST_AVAILABLE" = false ] && [ "$CPP_AVAILABLE" = false ]; then
    echo "❌ Cannot continue - no module schemas available"
    exit 1
fi

echo
echo "Step 2: Analyzing schemas..."
echo "============================"

# Extract sections for analysis
for section in typespace tables reducers named_types; do
    if [ "$RUST_AVAILABLE" = true ]; then
        analyze_schema_section "$RUST_SCHEMA" "$section" "rust"
    fi
    if [ "$CPP_AVAILABLE" = true ]; then
        analyze_schema_section "$CPP_SCHEMA" "$section" "cpp"
    fi
done

# Start analysis file
{
    echo "SpacetimeDB Module Schema Comparison"
    echo "===================================="
    echo "Generated on: $(date)"
    echo "Comparing: Rust SDK vs C++ SDK module schemas"
    echo
    
    # Basic file info
    echo "SCHEMA FILE SIZES"
    echo "-----------------"
    if [ "$RUST_AVAILABLE" = true ]; then
        echo "Rust schema: $(wc -c < "$RUST_SCHEMA" 2>/dev/null || echo "N/A") bytes"
    else
        echo "Rust schema: Not available"
    fi
    if [ "$CPP_AVAILABLE" = true ] && [ -f "$CPP_SCHEMA" ]; then
        echo "C++ schema:  $(wc -c < "$CPP_SCHEMA" 2>/dev/null || echo "N/A") bytes"
    else
        echo "C++ schema:  Not available"
    fi
    if [ "$RUST_AVAILABLE" = true ] && [ "$CPP_AVAILABLE" = true ]; then
        echo "Difference:  $(($(wc -c < "$CPP_SCHEMA") - $(wc -c < "$RUST_SCHEMA"))) bytes"
    fi
    echo
    
    echo "=================================================================="
    echo "SECTION 1: TYPESPACE (Anonymous types used internally)"
    echo "=================================================================="
    echo
    
    rust_typespace=0
    cpp_typespace=0
    
    # Count types in the typespace.types array
    if [ "$RUST_AVAILABLE" = true ] && [ -f "$RUST_SCHEMA" ]; then
        rust_typespace=$($PYTHON_CMD -c "import json; data=json.load(open('$RUST_SCHEMA')); ts=data.get('typespace', {}); types=ts.get('types', []); print(len(types))" 2>/dev/null || echo "0")
        echo "- Rust SDK: $rust_typespace types"
    else
        echo "- Rust SDK: Not available"
    fi
    
    if [ "$CPP_AVAILABLE" = true ] && [ -f "$CPP_SCHEMA" ]; then
        cpp_typespace=$($PYTHON_CMD -c "import json; data=json.load(open('$CPP_SCHEMA')); ts=data.get('typespace', {}); types=ts.get('types', []); print(len(types))" 2>/dev/null || echo "0")
        echo "- C++ SDK:  $cpp_typespace types"
    else
        echo "- C++ SDK:  Not available"
    fi
    
    if [ "$RUST_AVAILABLE" = true ] && [ "$CPP_AVAILABLE" = true ]; then
        echo "- Difference: $((cpp_typespace - rust_typespace))"
    fi
    echo
    
    # Analyze type patterns in typespace
    if [ "$CPP_AVAILABLE" = true ] || [ "$RUST_AVAILABLE" = true ]; then
        echo "Type patterns in typespace:"
        
        if [ -f "rust_typespace.json" ]; then
            rust_products=$(grep -c '"Product":' rust_typespace.json 2>/dev/null || echo "0")
            rust_sums=$(grep -c '"Sum":' rust_typespace.json 2>/dev/null || echo "0")
        else
            rust_products="N/A"
            rust_sums="N/A"
        fi
        
        if [ -f "cpp_typespace.json" ]; then
            cpp_products=$(grep -c '"Product":' cpp_typespace.json 2>/dev/null || echo "0")
            cpp_sums=$(grep -c '"Sum":' cpp_typespace.json 2>/dev/null || echo "0")
        else
            cpp_products="N/A"
            cpp_sums="N/A"
        fi
        
        echo "- Product types: Rust=$rust_products, C++=$cpp_products"
        echo "- Sum types: Rust=$rust_sums, C++=$cpp_sums"
        echo
    fi
    
    echo "=================================================================="
    echo "SECTION 2: TABLES"
    echo "=================================================================="
    echo
    # Count tables by counting table objects (look for '"name":' at the table level, not field level)
    # Each table starts with {"name": so count those
    rust_tables=$($PYTHON_CMD -c "import json; data=json.load(open('$RUST_SCHEMA')); print(len(data.get('tables', [])))" 2>/dev/null || echo "0")
    cpp_tables=$($PYTHON_CMD -c "import json; data=json.load(open('$CPP_SCHEMA')); print(len(data.get('tables', [])))" 2>/dev/null || echo "0")
    echo "Table counts:"
    echo "- Rust SDK: $rust_tables tables"
    echo "- C++ SDK:  $cpp_tables tables"
    echo "- Difference: $((cpp_tables - rust_tables))"
    
    if [ "$rust_tables" -eq "$cpp_tables" ] && [ "$rust_tables" -gt "0" ]; then
        echo "✅ Table count matches!"
    elif [ "$rust_tables" -ne "$cpp_tables" ]; then
        echo "⚠️  Table count mismatch!"
        echo
        echo "Table differences:"
        find_differences "$RUST_SCHEMA" "$CPP_SCHEMA" '"name": "[^"]*"'
    fi
    echo
    
    echo "=================================================================="
    echo "SECTION 3: REDUCERS"
    echo "=================================================================="
    echo
    # Count reducers properly using Python
    rust_reducers=$($PYTHON_CMD -c "import json; data=json.load(open('$RUST_SCHEMA')); print(len(data.get('reducers', [])))" 2>/dev/null || echo "0")
    cpp_reducers=$($PYTHON_CMD -c "import json; data=json.load(open('$CPP_SCHEMA')); print(len(data.get('reducers', [])))" 2>/dev/null || echo "0")
    echo "Reducer counts:"
    echo "- Rust SDK: $rust_reducers reducers"
    echo "- C++ SDK:  $cpp_reducers reducers"
    echo "- Difference: $((cpp_reducers - rust_reducers))"
    
    if [ "$rust_reducers" -eq "$cpp_reducers" ] && [ "$rust_reducers" -gt "0" ]; then
        echo "✅ Reducer count matches!"
    elif [ "$rust_reducers" -ne "$cpp_reducers" ]; then
        echo "⚠️  Reducer count mismatch!"
        echo
        echo "Reducer differences:"
        # Extract reducer names from both schemas
        sed -n '/"reducers":/,/^    ],$/p' "$RUST_SCHEMA" > rust_reducers_section.tmp
        sed -n '/"reducers":/,/^    ],$/p' "$CPP_SCHEMA" > cpp_reducers_section.tmp
        find_differences "rust_reducers_section.tmp" "cpp_reducers_section.tmp" '"name": "[^"]*"'
        rm -f rust_reducers_section.tmp cpp_reducers_section.tmp
    fi
    echo
    
    echo "=================================================================="
    echo "SECTION 4: NAMED TYPES (User-defined types)"
    echo "=================================================================="
    echo
    
    # Count named types in the typespace.types array (these are the actual named/anonymous types)
    rust_named_types=$($PYTHON_CMD -c "import json; data=json.load(open('$RUST_SCHEMA')); ts=data.get('typespace', {}); types=ts.get('types', []); print(len(types))" 2>/dev/null || echo "0")
    cpp_named_types=$($PYTHON_CMD -c "import json; data=json.load(open('$CPP_SCHEMA')); ts=data.get('typespace', {}); types=ts.get('types', []); print(len(types))" 2>/dev/null || echo "0")
    
    echo "Named type counts:"
    echo "- Rust SDK: $rust_named_types named types"
    echo "- C++ SDK:  $cpp_named_types named types"
    echo "- Difference: $((cpp_named_types - rust_named_types))"
    
    if [ "$rust_named_types" -ne "$cpp_named_types" ]; then
        echo
        echo "Named type differences:"
        # Extract just the type names for comparison
        grep -A2 '"name":' rust_named_types.json | grep '"name":' | grep -o '"[^"]*"$' | sort > rust_type_names.tmp
        grep -A2 '"name":' cpp_named_types.json | grep '"name":' | grep -o '"[^"]*"$' | sort > cpp_type_names.tmp
        
        echo "Only in Rust:"
        comm -23 rust_type_names.tmp cpp_type_names.tmp | head -10 | sed 's/^/  - /'
        
        echo "Only in C++:"
        comm -13 rust_type_names.tmp cpp_type_names.tmp | head -10 | sed 's/^/  - /'
        
        rm -f rust_type_names.tmp cpp_type_names.tmp
    fi
    echo
    
    # Analyze specific types
    echo "CRITICAL TYPE ANALYSIS"
    echo "====================="
    echo
    
    # Look for specific patterns that might indicate issues
    echo "Type index examples (showing potential misalignment):"
    echo "Rust:"
    grep -A3 '"ByteStruct"\|"EnumWithPayload"\|"UnitStruct"' rust_named_types.json 2>/dev/null | grep -E '"name":|"ty":' | head -6
    echo
    echo "C++:"
    grep -A3 '"ByteStruct"\|"EnumWithPayload"\|"UnitStruct"' cpp_named_types.json 2>/dev/null | grep -E '"name":|"ty":' | head -6
    echo
    
    # Check for potential duplicate registrations
    echo "Checking for duplicate type names:"
    echo "Rust duplicates:"
    grep '"name":' rust_named_types.json | grep -o '"[^"]*"$' | sort | uniq -c | grep -v '^ *1 ' | head -5
    echo "C++ duplicates:"
    grep '"name":' cpp_named_types.json | grep -o '"[^"]*"$' | sort | uniq -c | grep -v '^ *1 ' | head -5
    echo
    
    echo "=================================================================="
    echo "SUMMARY"
    echo "=================================================================="
    echo
    
    # Overall summary
    total_rust=$((rust_typespace + rust_tables + rust_reducers + rust_named_types))
    total_cpp=$((cpp_typespace + cpp_tables + cpp_reducers + cpp_named_types))
    
    echo "Total counts across all sections:"
    echo "- Rust SDK: $total_rust items"
    echo "- C++ SDK:  $total_cpp items"
    echo "- Difference: $((total_cpp - total_rust))"
    echo
    
    echo "Key findings:"
    if [ "$((cpp_typespace - rust_typespace))" -gt 0 ]; then
        echo "⚠️  C++ has $((cpp_typespace - rust_typespace)) extra anonymous types in typespace"
    fi
    
    if [ "$rust_tables" -ne "$cpp_tables" ]; then
        echo "⚠️  Table count differs by $((cpp_tables - rust_tables))"
    fi
    
    if [ "$rust_reducers" -ne "$cpp_reducers" ]; then
        echo "⚠️  Reducer count differs by $((cpp_reducers - rust_reducers))"
    fi
    
    if [ "$((cpp_named_types - rust_named_types))" -ne 0 ]; then
        echo "⚠️  Named type count differs by $((cpp_named_types - rust_named_types))"
    fi
    
    if [ "$rust_tables" -eq "$cpp_tables" ] && [ "$rust_reducers" -eq "$cpp_reducers" ]; then
        echo "✅ Table and reducer counts match perfectly"
    fi
    
} > "$ANALYSIS_FILE"

# Clean up temporary files
rm -f rust_*.json cpp_*.json 2>/dev/null

echo "✅ Module schema analysis complete!"
echo 
echo -e "${GREEN}📊 Summary:${NC}"

# Quick summary from the analysis
rust_named=$(grep "Rust SDK:.*named types" "$ANALYSIS_FILE" | tail -1 | grep -o '[0-9]\+' | head -1)
cpp_named=$(grep "C\+\+ SDK:.*named types" "$ANALYSIS_FILE" | tail -1 | grep -o '[0-9]\+' | head -1)

if [ -n "$rust_named" ] && [ -n "$cpp_named" ]; then
    echo "  • Rust schema: $rust_named named types"
    echo "  • C++ schema: $cpp_named named types"
    
    if [ "$rust_named" -eq "$cpp_named" ]; then
        echo -e "  • ${GREEN}✅ Named type count matches${NC}"
    else
        echo -e "  • ${YELLOW}⚠️  Named type count differs by $((cpp_named - rust_named))${NC}"  
    fi
fi

echo
echo -e "${BLUE}📁 Detailed analysis: $ANALYSIS_FILE${NC}"
echo -e "   File size: $(ls -lh "$ANALYSIS_FILE" | awk '{print $5}')"
echo