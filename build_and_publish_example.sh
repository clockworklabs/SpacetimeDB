#!/bin/bash

# Exit immediately if a command exits with a non-zero status.
set -e
# Treat unset variables as an error when substituting.
set -u

# --- Configuration ---
# Relative path to the example module directory from the project root (where this script is run)
readonly EXAMPLE_MODULE_DIR="examples/quickstart_cpp_kv"

# Relative path to the CMake toolchain file from the project root
readonly TOOLCHAIN_FILE_FROM_ROOT="toolchains/wasm_toolchain.cmake"

# Database name to publish to on SpacetimeDB
# Use first script argument if provided, else use default.
readonly DEFAULT_DATABASE_NAME="my_cpp_kv_store_test_db"
readonly DATABASE_NAME="${1:-${DEFAULT_DATABASE_NAME}}"

# --- Script Logic ---

echo "--- Starting C++ Example Build and Publish Script ---"

# 1. Verify necessary files and directories exist at project root
if [ ! -d "${EXAMPLE_MODULE_DIR}" ]; then
    echo "Error: Example module directory not found at ${EXAMPLE_MODULE_DIR} (relative to project root)."
    exit 1
fi
if [ ! -f "${TOOLCHAIN_FILE_FROM_ROOT}" ]; then
    echo "Error: Toolchain file not found at ${TOOLCHAIN_FILE_FROM_ROOT} (relative to project root)."
    exit 1
fi

# 2. Get the absolute path to the toolchain file
# This ensures CMake can find it regardless of where the build directory for the example is located.
TOOLCHAIN_FILE_ABS=""
# Try readlink -f first (GNU coreutils, common on Linux)
if command -v readlink >/dev/null && readlink -f "." >/dev/null 2>&1; then
    TOOLCHAIN_FILE_ABS="$(readlink -f "${TOOLCHAIN_FILE_FROM_ROOT}")"
# Fallback for macOS or other systems where readlink -f might not be available or behave differently
# This constructs an absolute path by cd'ing to the dirname and getting pwd.
elif command -v realpath >/dev/null; then # realpath is another option
    TOOLCHAIN_FILE_ABS="$(realpath "${TOOLCHAIN_FILE_FROM_ROOT}")"
else # More portable (but assumes dirname and basename are available)
    TOOLCHAIN_FILE_ABS="$(cd "$(dirname "${TOOLCHAIN_FILE_FROM_ROOT}")" && pwd)/$(basename "${TOOLCHAIN_FILE_FROM_ROOT}")"
fi


if [ ! -f "${TOOLCHAIN_FILE_ABS}" ]; then # Final check after attempting to make absolute
    echo "Error: Absolute path for toolchain file could not be determined or file does not exist: ${TOOLCHAIN_FILE_ABS}"
    echo "Attempted to resolve from relative path: ${TOOLCHAIN_FILE_FROM_ROOT}"
    exit 1
fi
echo "--- Using Toolchain File: ${TOOLCHAIN_FILE_ABS} ---"


# 3. Navigate into the example module directory
echo "--- Changing directory to ${EXAMPLE_MODULE_DIR} ---"
pushd "${EXAMPLE_MODULE_DIR}" > /dev/null


# 4. Parse MODULE_NAME from Cargo.toml within the example directory
if [ ! -f "Cargo.toml" ]; then
    echo "Error: Cargo.toml not found in $(pwd)"
    popd > /dev/null # Ensure we popd on error
    exit 1
fi
MODULE_NAME=$(grep -E '^name\s*=\s*".*"' Cargo.toml | sed -E 's/name\s*=\s*"(.*)"/\1/')

if [ -z "${MODULE_NAME}" ]; then
    echo "Error: Could not parse module name from Cargo.toml in $(pwd)"
    popd > /dev/null # Ensure we popd on error
    exit 1
fi
echo "--- Module Name (from Cargo.toml): ${MODULE_NAME} ---"


# 5. Define build and target directories relative to the example module directory (current directory)
readonly BUILD_DIR_IN_EXAMPLE="build"
readonly TARGET_DIR_IN_EXAMPLE="target/wasm32-unknown-unknown/release"
readonly WASM_FILE_PATH="${TARGET_DIR_IN_EXAMPLE}/${MODULE_NAME}.wasm"


# 6. Create build and target directories within the example module directory
echo "--- Creating build directory: ${BUILD_DIR_IN_EXAMPLE} ---"
mkdir -p "${BUILD_DIR_IN_EXAMPLE}"
echo "--- Ensuring target directory exists: ${TARGET_DIR_IN_EXAMPLE} ---"
mkdir -p "${TARGET_DIR_IN_EXAMPLE}"


# 7. Run CMake to configure the project
# The CMakeLists.txt within EXAMPLE_MODULE_DIR handles its own relative paths for SDK,
# expecting to find the SDK via add_subdirectory(../../../sdk ...)
echo "--- Configuring CMake for ${MODULE_NAME} ---"
# We are inside EXAMPLE_MODULE_DIR, so toolchain path is relative from here.
# TOOLCHAIN_FILE_ABS is already absolute, so it's fine.
cmake -B "${BUILD_DIR_IN_EXAMPLE}" -S . -DCMAKE_TOOLCHAIN_FILE="${TOOLCHAIN_FILE_ABS}"


# 8. Run CMake to build the project
# Output will go to ./${TARGET_DIR_IN_EXAMPLE}/${MODULE_NAME}.wasm
echo "--- Building WASM module: ${MODULE_NAME}.wasm ---"
cmake --build "${BUILD_DIR_IN_EXAMPLE}"


# 9. Check if the WASM file was created successfully
if [ ! -f "${WASM_FILE_PATH}" ]; then
    echo "Error: WASM file not found at $(pwd)/${WASM_FILE_PATH} after build."
    echo "Please check that MODULE_NAME in CMakeLists.txt matches the name in Cargo.toml ('${MODULE_NAME}')"
    echo "and that CMake output directories are set correctly."
    popd > /dev/null # Ensure we popd on error
    exit 1
else
    echo "--- WASM module built successfully: $(pwd)/${WASM_FILE_PATH} ---"
fi


# 10. Run spacetime publish
# Assumes `spacetime` CLI uses the module name from Cargo.toml found in the current directory (which is EXAMPLE_MODULE_DIR)
# to locate the .wasm file at the conventional path.
echo "--- Publishing module '${MODULE_NAME}' to SpacetimeDB as '${DATABASE_NAME}' ---"
# The command `spacetime publish <module_name_from_cargo>` implies that the CLI
# will look for Cargo.toml in the CWD, extract the name, and then find the .wasm file
# at target/wasm32-unknown-unknown/release/<module_name_from_cargo>.wasm
spacetime publish "${MODULE_NAME}" --name "${DATABASE_NAME}"


# 11. Return to the original directory
echo "--- Returning to original directory ---"
popd > /dev/null

echo ""
echo "--- Build and publish script for example '${MODULE_NAME}' completed successfully! ---"
echo "--- Published to database name: '${DATABASE_NAME}' ---"
