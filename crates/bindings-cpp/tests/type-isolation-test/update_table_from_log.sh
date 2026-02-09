#!/bin/bash

# Table updater - reads from test_log.txt and updates test_summary_live.txt

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

# Initialize module arrays
declare -a MODULES
declare -A BUILD_STATUS
declare -A PUBLISH_STATUS
declare -A WASM_SIZE
declare -A ERROR_MSG

# Initialize library status
LIBRARY_STATUS="⏳"
LIBRARY_ERROR=""

# Discover all modules
for cpp_file in test_modules/*.cpp; do
    if [ -f "$cpp_file" ]; then
        module=$(basename "${cpp_file%.cpp}")
        MODULES+=("$module")
        BUILD_STATUS["$module"]="⏳"
        PUBLISH_STATUS["$module"]="⏳"
        WASM_SIZE["$module"]="-"
        ERROR_MSG["$module"]=""
    fi
done

# Process existing log to reconstruct current state (if log exists)
if [ -f test_log.txt ]; then
    while IFS='|' read -r timestamp module field value; do
        # Skip empty lines
        [ -z "$module" ] && continue
        
        # Handle library status
        if [ "$module" = "LIBRARY" ]; then
            case "$field" in
                "build")
                    case "$value" in
                        "start") LIBRARY_STATUS="🔨" ;;
                        "pass") LIBRARY_STATUS="✅" ;;
                        "fail") LIBRARY_STATUS="❌" ;;
                    esac
                    ;;
                "error")
                    LIBRARY_ERROR="$value"
                    ;;
            esac
            continue
        fi
        
        # Update status based on field
        case "$field" in
            "build")
                case "$value" in
                    "start") BUILD_STATUS["$module"]="🔨" ;;
                    "pass") BUILD_STATUS["$module"]="✅" ;;
                    "fail") BUILD_STATUS["$module"]="❌" ;;
                esac
                ;;
            "publish")
                case "$value" in
                    "start") PUBLISH_STATUS["$module"]="📤" ;;
                    "pass") PUBLISH_STATUS["$module"]="✅" ;;
                    "fail") PUBLISH_STATUS["$module"]="❌" ;;
                    "skip") PUBLISH_STATUS["$module"]="⏭️" ;;
                esac
                ;;
            "size")
                WASM_SIZE["$module"]="$value"
                ;;
            "error")
                ERROR_MSG["$module"]="$value"
                ;;
        esac
    done < test_log.txt
fi

# Sort modules
IFS=$'\n' MODULES=($(sort <<<"${MODULES[*]}"))
unset IFS

# Function to render table
render_table() {
    {
        echo "============================================================"
        echo "SpacetimeDB C++ Library Type Isolation Test - Live Progress"
        echo "Generated: $(date)"
        echo "Last Update: $(date +%H:%M:%S)"
        echo "============================================================"
        echo
        echo "Test Environment:"
        echo "  Directory: $SCRIPT_DIR"
        echo "  Total Modules: ${#MODULES[@]}"
        echo
        echo "============================================================"
        echo "Live Test Status Table:"
        echo "============================================================"
        echo
        printf "%-27s | %-10s | %-12s | %-10s | %s\n" "Module" "Build" "Publish" "WASM Size" "Error"
        echo "----------------------------|-----------|-------------|-----------|------------------------------------"
        
        # Show library build status first
        # Truncate library error if too long (for table display)
        local lib_error_display="$LIBRARY_ERROR"
        if [ ${#lib_error_display} -gt 200 ]; then
            lib_error_display="${lib_error_display:0:197}..."
        fi
        printf "%-27s | %-10s | %-12s | %-10s | %s\n" "📚 Library" "$LIBRARY_STATUS" "-" "-" "$lib_error_display"
        echo "----------------------------|-----------|-------------|-----------|------------------------------------"
        
        for module in "${MODULES[@]}"; do
            build="${BUILD_STATUS[$module]}"
            publish="${PUBLISH_STATUS[$module]}"
            size="${WASM_SIZE[$module]}"
            error="${ERROR_MSG[$module]}"
            
            # Truncate error if too long (but show more context for table display)
            if [ ${#error} -gt 250 ]; then
                error="${error:0:247}..."
            fi
            
            printf "%-27s | %-10s | %-12s | %-10s | %s\n" "$module" "$build" "$publish" "$size" "$error"
        done
        
        echo
        echo "============================================================"
        echo "Legend:"
        echo "  ⏳ = Pending    🔨 = Building    📤 = Publishing"
        echo "  ✅ = Passed     ❌ = Failed      ⏭️  = Skipped"
        echo "============================================================"
        
        # Progress counter
        local completed=0
        local building=0
        local publishing=0
        local total=${#MODULES[@]}
        
        for module in "${MODULES[@]}"; do
            case "${BUILD_STATUS[$module]}" in
                "✅"|"❌") ((completed++)) ;;
                "🔨") ((building++)) ;;
            esac
            case "${PUBLISH_STATUS[$module]}" in
                "📤") ((publishing++)) ;;
            esac
        done
        
        echo
        echo "Progress: $completed/$total builds complete, $building building, $publishing publishing"
        
        # Add statistics if all complete
        if [ $completed -eq $total ]; then
            echo
            echo "============================================================"
            echo "Final Statistics:"
            echo "============================================================"
            
            local passed=0
            local build_fail=0
            local publish_fail=0
            
            for module in "${MODULES[@]}"; do
                if [ "${BUILD_STATUS[$module]}" = "✅" ] && [ "${PUBLISH_STATUS[$module]}" = "✅" ]; then
                    ((passed++))
                elif [ "${BUILD_STATUS[$module]}" = "❌" ]; then
                    ((build_fail++))
                elif [ "${PUBLISH_STATUS[$module]}" = "❌" ]; then
                    ((publish_fail++))
                fi
            done
            
            echo "Total modules: $total"
            echo "Fully passed: $passed"
            echo "Build failures: $build_fail"
            echo "Publish failures: $publish_fail"
            echo "Success rate: $(( passed * 100 / total ))%"
            echo "============================================================"
        fi
    } > test_summary_live.txt
}

# Track last processed line - count lines already in log
if [ -f test_log.txt ]; then
    LAST_LINE=$(wc -l < test_log.txt)
else
    LAST_LINE=0
fi

# Initial render
render_table

# Continuously read log and update table
while true; do
    # Check if log file exists
    if [ ! -f test_log.txt ]; then
        sleep 0.5
        continue
    fi
    
    # Read new lines from log
    CURRENT_LINES=$(wc -l < test_log.txt)
    
    if [ $CURRENT_LINES -gt $LAST_LINE ]; then
        # Process new lines - use process substitution to avoid subshell
        while IFS='|' read -r timestamp module field value; do
            # Skip empty lines
            [ -z "$module" ] && continue
            
            # Check for completion signal
            if [ "$module" = "COMPLETE" ]; then
                render_table
                exit 0
            fi
            
            # Handle library status
            if [ "$module" = "LIBRARY" ]; then
                case "$field" in
                    "build")
                        case "$value" in
                            "start") LIBRARY_STATUS="🔨" ;;
                            "pass") LIBRARY_STATUS="✅" ;;
                            "fail") LIBRARY_STATUS="❌" ;;
                        esac
                        ;;
                    "error")
                        LIBRARY_ERROR="$value"
                        ;;
                esac
            else
                # Update status based on field
                case "$field" in
                    "build")
                        case "$value" in
                            "start") BUILD_STATUS["$module"]="🔨" ;;
                            "pass") BUILD_STATUS["$module"]="✅" ;;
                            "fail") BUILD_STATUS["$module"]="❌" ;;
                        esac
                        ;;
                    "publish")
                        case "$value" in
                            "start") PUBLISH_STATUS["$module"]="📤" ;;
                            "pass") PUBLISH_STATUS["$module"]="✅" ;;
                            "fail") PUBLISH_STATUS["$module"]="❌" ;;
                            "skip") PUBLISH_STATUS["$module"]="⏭️" ;;
                        esac
                        ;;
                    "size")
                        WASM_SIZE["$module"]="$value"
                        ;;
                    "error")
                        # Store full error for log
                        ERROR_MSG["$module"]="$value"
                        
                        # Extract cleaner error for table display
                        # Look for key error patterns and extract the important part
                        clean_error=""
                        if [[ "$value" =~ "Error: Errors occurred:" ]]; then
                            # Extract the main error message after "Error: Errors occurred:"
                            # Get the part between "Error: Errors occurred:" and "Caused by:"
                            clean_error=$(echo "$value" | sed 's/.*Error: Errors occurred: //' | sed 's/Caused by:.*//' | sed 's/[[:space:]]*$//' | head -1)
                            if [ -n "$clean_error" ]; then
                                clean_error="Error: $clean_error"
                            fi
                        elif [[ "$value" =~ "error: static assertion failed:" ]]; then
                            # Extract the assertion message with more context
                            clean_error=$(echo "$value" | grep -o 'error: static assertion failed: [^.]*' | head -1)
                        elif [[ "$value" =~ "error:" ]]; then
                            # Generic error - try to extract first error line with more context
                            clean_error=$(echo "$value" | grep -o "error:[^|]*" | head -1 | sed 's/[[:space:]]*$//')
                        elif [[ "$value" =~ "Error:" ]]; then
                            # Spacetime error - extract the main message with more context
                            clean_error=$(echo "$value" | sed 's/.*\(Error:[^|]*\).*/\1/' | sed 's/[[:space:]]*$//' | head -1)
                        fi
                        
                        # Use clean error if we extracted one
                        if [ -n "$clean_error" ]; then
                            ERROR_MSG["$module"]="$clean_error"
                        fi
                        ;;
                esac
            fi
        done < <(tail -n +$((LAST_LINE + 1)) test_log.txt)
        
        # Update last processed line
        LAST_LINE=$CURRENT_LINES
        
        # Render updated table
        render_table
    fi
    
    # Small delay to avoid CPU spinning
    sleep 0.2
done