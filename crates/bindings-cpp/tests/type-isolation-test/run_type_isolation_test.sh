#!/bin/bash

# Main test runner with live table updates
# Uses test_log.txt for events and test_summary_live.txt for the table

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$SCRIPT_DIR/tmp"
cd "$SCRIPT_DIR"

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

# Parse arguments
MAX_PARALLEL=16
TEST_MODE="v10-regression"
for arg in "$@"; do
    case "$arg" in
        --v10-regression)
            TEST_MODE="v10-regression"
            ;;
        --v9)
            TEST_MODE="v9"
            ;;
        ''|*[!0-9]*)
            ;;
        *)
            MAX_PARALLEL="$arg"
            ;;
    esac
done

# Clear previous state
echo "Starting fresh test run..."
rm -f test_log.txt test_summary_live.txt
touch test_log.txt

# Ensure spacetime is running
check_server_running() {
    curl -s http://127.0.0.1:3000/health >/dev/null 2>&1
}

if ! check_server_running; then
    echo "Starting SpacetimeDB server..."
    nohup spacetime start > "$TMP_DIR/spacetime.log" 2>&1 &
    
    echo "Waiting for server to start..."
    for i in {1..30}; do
        if check_server_running; then
            echo "Server started successfully!"
            break
        fi
        sleep 1
        if [ $i -eq 30 ]; then
            echo "Error: Server failed to start after 30 seconds"
            exit 1
        fi
    done
else
    echo "SpacetimeDB server already running"
fi

# Discover modules
declare -a MODULES
for cpp_file in test_modules/*.cpp; do
    if [ -f "$cpp_file" ]; then
        module_name=$(basename "${cpp_file%.cpp}")
        MODULES+=("$module_name")
    fi
done

# Optional focused regression selection for V10 behavior checks
if [ "$TEST_MODE" = "v10-regression" ]; then
    declare -a FILTERED_MODULES=()
    for module in "${MODULES[@]}"; do
        case "$module" in
            test_multicolumn_index_valid|error_multicolumn_missing_field|error_default_missing_field|error_circular_ref)
                FILTERED_MODULES+=("$module")
                ;;
        esac
    done
    MODULES=("${FILTERED_MODULES[@]}")
fi

# Sort modules
IFS=$'\n' MODULES=($(sort <<<"${MODULES[*]}"))
unset IFS

echo "========================================="
echo "Testing ${#MODULES[@]} modules"
echo "Mode: ${TEST_MODE}"
echo "Parallelism: ${MAX_PARALLEL} (use ./run_type_isolation_test.sh <num> to change)"
echo "Use --v9 to run the broader legacy/full module suite."
echo "========================================="
echo "Monitor table with: watch -n 1 cat test_summary_live.txt"
echo "Monitor log with: tail -f test_log.txt"
echo "========================================="

# Create results directory
mkdir -p results
mkdir -p "$TMP_DIR"

# Function to write log entry (always write to script dir)
write_log() {
    echo "$(date +%s)|$1|$2|$3" >> "$SCRIPT_DIR/test_log.txt"
}

# Start the table updater in background
echo "Starting table updater..."
./update_table_from_log.sh &
TABLE_UPDATER_PID=$!

# Array to track build job PIDs
declare -a BUILD_PIDS=()

# Give it a second to start
sleep 1

# Pre-build the SpacetimeDB library once
NUM_CORES=$(nproc 2>/dev/null || sysctl -n hw.ncpu 2>/dev/null || echo 4)
echo "========================================="
echo "Pre-building SpacetimeDB library (using $NUM_CORES cores)..."
echo "========================================="

write_log "LIBRARY" "build" "start"

LIBRARY_BUILD_DIR="library_build"
rm -rf "$LIBRARY_BUILD_DIR"
mkdir -p "$LIBRARY_BUILD_DIR"
cd "$LIBRARY_BUILD_DIR"

LIBRARY_ERROR_FILE="$TMP_DIR/library_build_error.txt"
if ! $EMCMAKE_CMD cmake ../../../ > "$LIBRARY_ERROR_FILE" 2>&1; then
    write_log "LIBRARY" "build" "fail"
    ERROR_MSG=$(grep -E "(error:|Error:|ERROR:)" "$LIBRARY_ERROR_FILE" 2>/dev/null | head -5 | tr '\n' ' ')
    if [ -z "$ERROR_MSG" ]; then
        ERROR_MSG=$(tail -20 "$LIBRARY_ERROR_FILE" 2>/dev/null | tr '\n' ' ')
    fi
    ERROR_MSG="${ERROR_MSG:0:500}"
    write_log "LIBRARY" "error" "${ERROR_MSG:-Failed to configure library build}"
    echo "❌ Failed to configure library build"
    echo "Error details:"
    tail -30 "$LIBRARY_ERROR_FILE"
    echo "========================================="
    echo "Library build failed. Updating table with error details..."
    # Force table updater to process the error
    sleep 3  # Give more time for table updater to process
    # Send SIGUSR1 to force immediate update (if the script supports it)
    kill -USR1 $TABLE_UPDATER_PID 2>/dev/null || true
    sleep 1  # Wait for final update
    echo "Check test_summary_live.txt for error details."
    kill $TABLE_UPDATER_PID 2>/dev/null
    exit 1
fi

# Use all available cores for parallel compilation (already set above)
if ! cmake --build . -j$NUM_CORES > "$LIBRARY_ERROR_FILE" 2>&1; then
    # Extract more comprehensive error messages FIRST
    ERROR_MSG=$(grep -E "(error:|Error:|ERROR:|undefined reference|fatal error)" "$LIBRARY_ERROR_FILE" 2>/dev/null | head -10 | tr '\n' ' ')
    if [ -z "$ERROR_MSG" ]; then
        ERROR_MSG=$(tail -50 "$LIBRARY_ERROR_FILE" 2>/dev/null | tr '\n' ' ')
    fi
    ERROR_MSG="${ERROR_MSG:0:800}"  # Allow more space for library errors
    
    # Write to log BEFORE any output
    write_log "LIBRARY" "build" "fail"
    write_log "LIBRARY" "error" "${ERROR_MSG:-Failed to build library}"
    
    # Force flush the log file
    sync
    
    echo "❌ Failed to build library"
    echo "Error details:"
    tail -50 "$LIBRARY_ERROR_FILE"
    echo "========================================="
    echo "Library build failed. Updating table with error details..."
    
    # Give table updater more time to read the log and update
    sleep 4  # Increased sleep time
    
    echo "Check test_summary_live.txt for error details."
    
    # Gracefully terminate table updater
    kill -TERM $TABLE_UPDATER_PID 2>/dev/null
    wait $TABLE_UPDATER_PID 2>/dev/null
    
    exit 1
fi
rm -f "$LIBRARY_ERROR_FILE"

cd ..
write_log "LIBRARY" "build" "pass"
echo "✅ Library built successfully"

# Create the expected directory structure and copy the library
mkdir -p "$LIBRARY_BUILD_DIR/spacetimedb_lib"
cp "$LIBRARY_BUILD_DIR/libspacetimedb_cpp_library.a" "$LIBRARY_BUILD_DIR/spacetimedb_lib/"

echo "========================================="

# Export library paths for module builds
export SPACETIMEDB_LIBRARY_DIR="$(pwd)/$LIBRARY_BUILD_DIR/spacetimedb_lib"
export SPACETIMEDB_INCLUDE_DIR="$(pwd)/../../include"

get_expected_failure_marker() {
    local module=$1
    case "$module" in
        error_multicolumn_missing_field)
            echo "ERROR_CONSTRAINT_REGISTRATION_FIELD_NOT_FOUND"
            ;;
        error_default_missing_field)
            echo "ERROR_CONSTRAINT_REGISTRATION_FIELD_NOT_FOUND"
            ;;
        error_circular_ref)
            echo "ERROR_CIRCULAR_REFERENCE_"
            ;;
        *)
            echo ""
            ;;
    esac
}

# Function to publish module in background
publish_module() {
    local module=$1
    local wasm="test_modules/build_${module}/lib.wasm"
    
    if [ ! -f "$wasm" ]; then
        write_log "$module" "publish" "skip"
        return
    fi
    
    # Get size
    local size=$(stat -c%s "$wasm" 2>/dev/null || echo "0")
    local size_kb=$((size / 1024))
    write_log "$module" "size" "${size_kb}KB"
    
    # Update to publishing
    write_log "$module" "publish" "start"
    
    local db_name=$(echo "testmod-${module}" | sed 's/_/-/g')
    echo "  📤 Publishing $module as $db_name..."
    local PUBLISH_ERROR_FILE="$TMP_DIR/publish_error_${module}.txt"
    timeout 60 spacetime publish --bin-path "$wasm" -c "$db_name" -y >"$PUBLISH_ERROR_FILE" 2>&1
    local publish_exit=$?

    local expected_marker
    expected_marker=$(get_expected_failure_marker "$module")

    local has_publish_error=0
    if grep -q "Error: Errors occurred:" "$PUBLISH_ERROR_FILE" 2>/dev/null || \
       grep -q "HTTP status server error" "$PUBLISH_ERROR_FILE" 2>/dev/null || \
       grep -q "invalid ref:" "$PUBLISH_ERROR_FILE" 2>/dev/null; then
        has_publish_error=1
    fi

    local publish_success=0
    if [ $publish_exit -eq 0 ] && [ $has_publish_error -eq 0 ]; then
        publish_success=1
    fi

    if [ -n "$expected_marker" ]; then
        if [ $publish_success -eq 0 ] && grep -q "$expected_marker" "$PUBLISH_ERROR_FILE" 2>/dev/null; then
            write_log "$module" "publish" "pass"
            write_log "$module" "error" "Expected publish failure validated: $expected_marker"
            echo "  ✅ Expected publish failure validated: $module ($expected_marker)"
            rm -f "$PUBLISH_ERROR_FILE"
            return
        fi

        write_log "$module" "publish" "fail"
        local EXPECTED_MSG="Expected publish failure with marker '$expected_marker'"
        ERROR_MSG=$(sed -n '/Error:/,+10p' "$PUBLISH_ERROR_FILE" 2>/dev/null | tr '\n' ' ')
        if [ -z "$ERROR_MSG" ]; then
            ERROR_MSG=$(tail -n 15 "$PUBLISH_ERROR_FILE" 2>/dev/null | tr '\n' ' ')
        fi
        ERROR_MSG="$EXPECTED_MSG | Actual: ${ERROR_MSG:0:400}"
        write_log "$module" "error" "$ERROR_MSG"
        echo "  ❌ Expected failure marker missing for $module"
        rm -f "$PUBLISH_ERROR_FILE"
        return
    fi

    if [ $publish_success -eq 1 ]; then
        write_log "$module" "publish" "pass"
        echo "  ✅ Published $module"
        rm -f "$PUBLISH_ERROR_FILE"
    else
        write_log "$module" "publish" "fail"
        # Capture more comprehensive error message
        # Get the full error starting from "Error:" line and including several lines after
        ERROR_MSG=$(sed -n '/Error:/,+10p' "$PUBLISH_ERROR_FILE" 2>/dev/null | tr '\n' ' ')
        if [ -z "$ERROR_MSG" ]; then
            # Try getting the last 15 lines which usually contain the actual error
            ERROR_MSG=$(tail -n 15 "$PUBLISH_ERROR_FILE" 2>/dev/null | tr '\n' ' ')
        fi
        if [ -z "$ERROR_MSG" ]; then
            # Last resort - get the whole file if it's small
            ERROR_MSG=$(cat "$PUBLISH_ERROR_FILE" 2>/dev/null | tr '\n' ' ')
        fi
        # Truncate to 500 chars (much more than before)
        ERROR_MSG="${ERROR_MSG:0:500}"
        write_log "$module" "error" "${ERROR_MSG:-Publish failed}"
        echo "  ❌ Publish failed: $module"
        rm -f "$PUBLISH_ERROR_FILE"
    fi
}

# Function to build a module
build_module() {
    local module=$1
    local count=$2
    local total=$3
    
    echo "[$count/$total] Building $module..."
    
    # Log build start
    write_log "$module" "build" "start"
    
    # Create build directory under test_modules
    BUILD_DIR="test_modules/build_${module}"
    rm -rf "$BUILD_DIR"
    mkdir -p "$BUILD_DIR"
    
    cd "$BUILD_DIR"
    
    # Try to build - use the module-specific CMake with pre-built library
    BUILD_ERROR_FILE="$TMP_DIR/build_error_${module}.txt"
    
    # Copy the module CMakeLists
    cp ../../CMakeLists.module.txt CMakeLists.txt
    
    if $EMCMAKE_CMD cmake . \
        -DMODULE_SOURCE="../../test_modules/${module}.cpp" \
        -DOUTPUT_NAME="${module}" \
        -DSPACETIMEDB_LIBRARY_DIR="$SPACETIMEDB_LIBRARY_DIR" \
        -DSPACETIMEDB_INCLUDE_DIR="$SPACETIMEDB_INCLUDE_DIR" > /dev/null 2>"$BUILD_ERROR_FILE"; then
        
        if cmake --build . > /dev/null 2>"$BUILD_ERROR_FILE"; then
            cd ../..
            write_log "$module" "build" "pass"
            echo "  ✅ Build successful: $module"
            rm -f "$BUILD_ERROR_FILE"
            # Start publish in background
            publish_module "$module" &
        else
            cd ../..
            write_log "$module" "build" "fail"
            # Capture first 1000 chars of error
            ERROR_MSG=$(head -n 10 "$BUILD_ERROR_FILE" 2>/dev/null | tr '\n' ' ' | cut -c1-1000)
            write_log "$module" "error" "${ERROR_MSG:-Build failed}"
            echo "  ❌ Build failed: $module"
            rm -f "$BUILD_ERROR_FILE"
        fi
    else
        cd ../..
        write_log "$module" "build" "fail"
        # Capture first 1000 chars of error
        ERROR_MSG=$(head -n 10 "$BUILD_ERROR_FILE" 2>/dev/null | tr '\n' ' ' | cut -c1-1000)
        write_log "$module" "error" "${ERROR_MSG:-CMake failed}"
        echo "  ❌ CMake configuration failed: $module"
        rm -f "$BUILD_ERROR_FILE"
    fi
}

# Parallel build management with configurable parallelism
echo "Building modules with parallelism of $MAX_PARALLEL..."

# Build modules maintaining constant parallelism
COUNT=0
TOTAL=${#MODULES[@]}
for module in "${MODULES[@]}"; do
    ((COUNT++))
    
    # Wait if we have max parallel jobs running
    while (( $(jobs -r | wc -l) >= MAX_PARALLEL )); do
        sleep 0.2
    done
    
    # Start new build job
    build_module "$module" "$COUNT" "$TOTAL" &
    BUILD_PIDS+=($!)
    
    # Small delay to avoid race conditions
    sleep 0.05
done

# Wait for all remaining builds to complete (excluding table updater)
for pid in "${BUILD_PIDS[@]}"; do
    wait $pid 2>/dev/null
done

echo ""
echo "Waiting for any remaining background jobs..."
# Give a moment for any publish jobs to complete
sleep 2

# Check if there are any publish jobs still running (excluding table updater)
for i in {1..10}; do
    # Count background jobs excluding the table updater
    JOB_COUNT=$(jobs -p | grep -v "^$TABLE_UPDATER_PID$" | wc -l)
    if [ $JOB_COUNT -eq 0 ]; then
        break
    fi
    sleep 1
done

# Signal completion
write_log "COMPLETE" "COMPLETE" "COMPLETE"

# Wait a bit for table updater to finish
sleep 2

# Kill table updater
kill $TABLE_UPDATER_PID 2>/dev/null

echo
echo "========================================="
echo "Test Complete!"
echo "========================================="
echo "Final table in: test_summary_live.txt"
echo "Log file: test_log.txt"
