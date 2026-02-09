#!/bin/bash

# Script to compare Rust and C++ module library generated clients
# Ignores version comment lines for cleaner comparison

OUTPUT_FILE="client_diff_analysis.txt"
RUST_DIR="rust-sdk-test"
CPP_DIR="cpp-sdk-test"

echo "SpacetimeDB Module Library Client Generation Comparison" > $OUTPUT_FILE
echo "=======================================================" >> $OUTPUT_FILE
echo "Generated on: $(date)" >> $OUTPUT_FILE
echo "Comparing: $RUST_DIR vs $CPP_DIR" >> $OUTPUT_FILE
echo "" >> $OUTPUT_FILE

# File count comparison
echo "FILE COUNT COMPARISON" >> $OUTPUT_FILE
echo "--------------------" >> $OUTPUT_FILE
RUST_COUNT=$(find $RUST_DIR -name "*.rs" | wc -l)
CPP_COUNT=$(find $CPP_DIR -name "*.rs" | wc -l)
echo "Rust module library files: $RUST_COUNT" >> $OUTPUT_FILE
echo "C++ module library files:  $CPP_COUNT" >> $OUTPUT_FILE
echo "Difference:     $((CPP_COUNT - RUST_COUNT))" >> $OUTPUT_FILE
echo "" >> $OUTPUT_FILE

# Files only in one directory
echo "FILES UNIQUE TO EACH MODULE LIBRARY" >> $OUTPUT_FILE
echo "-----------------------------------" >> $OUTPUT_FILE
echo "Files only in Rust module library:" >> $OUTPUT_FILE
comm -23 <(cd $RUST_DIR && find . -name "*.rs" | sort) <(cd $CPP_DIR && find . -name "*.rs" | sort) >> $OUTPUT_FILE
echo "" >> $OUTPUT_FILE
echo "Files only in C++ module library:" >> $OUTPUT_FILE
comm -13 <(cd $RUST_DIR && find . -name "*.rs" | sort) <(cd $CPP_DIR && find . -name "*.rs" | sort) >> $OUTPUT_FILE
echo "" >> $OUTPUT_FILE

# Common files
echo "COMMON FILES COMPARISON" >> $OUTPUT_FILE
echo "----------------------" >> $OUTPUT_FILE
COMMON_FILES=$(comm -12 <(cd $RUST_DIR && find . -name "*.rs" | sort) <(cd $CPP_DIR && find . -name "*.rs" | sort))
echo "Number of common files: $(echo "$COMMON_FILES" | wc -l)" >> $OUTPUT_FILE
echo "" >> $OUTPUT_FILE

# Function to get meaningful diff content only
get_meaningful_diff() {
    local file1="$1"
    local file2="$2"
    
    # Create temp files without version comment lines
    local temp1=$(mktemp)
    local temp2=$(mktemp)
    
    # Remove version comment line (line starting with "// This was generated using")
    grep -v "^// This was generated using spacetimedb cli version" "$file1" > "$temp1"
    grep -v "^// This was generated using spacetimedb cli version" "$file2" > "$temp2"
    
    # Check if files are identical after filtering version comments
    if diff -q "$temp1" "$temp2" > /dev/null 2>&1; then
        # Files are identical after filtering
        rm "$temp1" "$temp2"
        return 1  # No meaningful differences
    else
        # Files have differences - extract meaningful change lines
        diff -u "$temp1" "$temp2" 2>/dev/null | grep -E '^[+-]' | grep -v -E '^[+-]{3}' | head -20
        rm "$temp1" "$temp2"
        return 0  # Has meaningful differences
    fi
}

# Detailed diff of common files
echo "DETAILED DIFFS OF COMMON FILES" >> $OUTPUT_FILE
echo "==============================" >> $OUTPUT_FILE
echo "(Version comment lines are ignored)" >> $OUTPUT_FILE
echo "" >> $OUTPUT_FILE

IDENTICAL_COUNT=0
DIFFERENT_COUNT=0

for file in $COMMON_FILES; do
    # Get meaningful differences for this file
    meaningful_diff=$(get_meaningful_diff "$RUST_DIR/$file" "$CPP_DIR/$file")
    diff_result=$?
    
    if [ $diff_result -eq 0 ] && [ -n "$meaningful_diff" ]; then
        # File has meaningful differences
        ((DIFFERENT_COUNT++))
        echo "" >> $OUTPUT_FILE
        echo "DIFF: $file" >> $OUTPUT_FILE
        echo "$(printf '=%.0s' {1..50})" >> $OUTPUT_FILE
        echo "$meaningful_diff" >> $OUTPUT_FILE
        echo "" >> $OUTPUT_FILE
    else
        # File is identical after filtering
        ((IDENTICAL_COUNT++))
    fi
done

# Statistics summary with accurate counts
echo "STATISTICS SUMMARY" >> $OUTPUT_FILE
echo "==================" >> $OUTPUT_FILE
echo "Total files compared: $(echo "$COMMON_FILES" | wc -l)" >> $OUTPUT_FILE
echo "Identical files (ignoring version): $IDENTICAL_COUNT" >> $OUTPUT_FILE
echo "Different files: $DIFFERENT_COUNT" >> $OUTPUT_FILE
echo "" >> $OUTPUT_FILE

echo "Analysis complete. Results written to: $OUTPUT_FILE"
echo "File size: $(du -h $OUTPUT_FILE | cut -f1)"
echo ""
echo "Summary:"
echo "  - Identical files (ignoring version): $IDENTICAL_COUNT"
echo "  - Different files: $DIFFERENT_COUNT"