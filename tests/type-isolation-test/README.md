# SpacetimeDB C++ Type Isolation Test Suite

Comprehensive test framework for systematically testing individual C++ types with the SpacetimeDB C++ library. Features parallel builds, real-time progress monitoring, and automated constraint validation testing.

## Quick Start

```bash
# Run all tests with default parallelism (16 builds)
./run_type_isolation_test.sh

# Run with custom parallelism
./run_type_isolation_test.sh 8

# Monitor progress in real-time (separate terminal)
watch -n 1 cat test_summary_live.txt
```

## Features

- **Parallel Execution**: Configurable parallelism (default: 16 concurrent builds)
- **Live Progress Monitoring**: Real-time status table with build/publish progress
- **Optimized Builds**: Pre-builds library once, 2-3x faster than individual builds
- **Smart Error Capture**: Clean error messages (up to 250 characters) in status table
- **Auto-discovery**: Automatically tests all `.cpp` files in `test_modules/`
- **State Recovery**: Resumes from previous state if interrupted

## Test Categories

### Basic Types (module01-12)
- **Primitives**: U8-U256, I8-I256, F32/F64, Bool, String
- **Special Types**: Identity, Timestamp, ConnectionId
- **Complex Types**: Enums, structs, vectors, optionals
- **Constraints**: Unique constraints and indexes

### Constraint Validation (error_*)
Tests that **intentionally fail** to validate error detection:
- `error_autoinc_non_integer`: Compile-time error for AutoIncrement on non-integers
- `error_invalid_index`: Compile-time error for Index on non-filterable types  
- `error_multiple_pk`: Runtime error for multiple primary keys
- `error_non_spacetimedb_type`: Compile-time error for unsupported types
- `error_circular_ref`: Runtime error for circular type references
- `error_scheduled_id_pk`: Runtime error for invalid scheduled tables

### Debug & Edge Cases (debug_*, test_*)
- Type combinations and interactions
- Reducer and table scenarios
- Specific debugging modules

## Output

- **`test_summary_live.txt`**: Live status table
- **`test_log.txt`**: Detailed build/publish log with timestamps
- **`test_modules/build_*/`**: Individual build directories
- **`library_build/`**: Pre-built shared library

## Status Table Format

```
Module                    | Build | Publish | WASM Size | Error
--------------------------|-------|---------|-----------|------------------
debug_large_struct        | ✅    | ✅      | 299KB     | 
error_multiple_pk         | ✅    | ❌      | 306KB     | Multiple primary keys
module01_basic_unsigned   | 🔨    | ⏳      | -         | 
```

**Status Indicators:** ⏳ Pending, 🔨 Building, 📤 Publishing, ✅ Passed, ❌ Failed, ⏭️ Skipped

## Performance

- **Total time**: ~1 minute for 57 modules
- **Library pre-build**: ~10 seconds  
- **Module builds**: 5-13 seconds each (vs 20-30 without optimization)
- **Current success rate**: 89% (51/57 passing, 6 intentional failures)

## Requirements

- SpacetimeDB CLI and server
- Emscripten (emcc, emcmake)
- CMake 3.16+, C++20 compiler
- Bash 4.0+ with associative arrays

## Usage

**Standard workflow:**
```bash
cd crates/bindings-cpp/tests/type-isolation-test
./run_type_isolation_test.sh
```

**Monitor progress:**
```bash
watch -n 1 cat test_summary_live.txt
```

**Check detailed errors:**
```bash
grep "error" test_log.txt
```

## Troubleshooting

**Server issues:**
```bash
curl -s http://127.0.0.1:3000/health  # Check server
spacetime start                        # Start if needed
```

**Build failures:**
- Check error messages in live table (250 char limit)
- Review `test_log.txt` for full details
- Build artifacts in `test_modules/build_<module>/`

**Stuck updates:**
```bash
pkill -f update_table_from_log  # Kill orphaned processes
./run_type_isolation_test.sh    # Restart
```

## Adding Tests

1. Create `.cpp` file in `test_modules/` following naming: `test_<category>_<specific>.cpp`
2. Include minimal code isolating the type being tested
3. Run test suite to verify

**Basic template:**
```cpp
#include <spacetimedb.h>
using namespace SpacetimeDb;

struct TestTable { /* fields */ };
SPACETIMEDB_STRUCT(TestTable, /* field list */)
SPACETIMEDB_TABLE(TestTable, test_table, Public)

SPACETIMEDB_INIT(init, ReducerContext ctx) {
    LOG_INFO("Test module initialized");
    return Ok();
}
```

## Integration

**CI/CD example:**
```bash
./run_type_isolation_test.sh
SUCCESS_RATE=$(grep "Success rate:" test_summary_live.txt | grep -o '[0-9]*%')
[[ "${SUCCESS_RATE}" == "89%" ]] || exit 1  # Expected rate with intentional failures
```

The test suite validates the C++ bindings type system by testing individual types in isolation, ensuring reliable constraint validation and error detection.