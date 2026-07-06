#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BINDINGS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
INCLUDE_DIR="$BINDINGS_ROOT/include"
BUILD_ROOT="$SCRIPT_DIR/build"
LIBRARY_BUILD_DIR="$BUILD_ROOT/library"
LIBRARY_LOG_DIR="$BUILD_ROOT/logs"
TEMPLATE_PATH="$SCRIPT_DIR/CMakeLists.module.txt"

SUITE="http-handlers"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --suite)
            SUITE="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

if [[ "$SUITE" != "http-handlers" ]]; then
    echo "Unsupported suite: $SUITE" >&2
    exit 1
fi

if command -v emcmake >/dev/null 2>&1; then
    EMCMAKE_CMD="emcmake"
elif command -v emcmake.bat >/dev/null 2>&1; then
    EMCMAKE_CMD="emcmake.bat"
else
    echo "Unable to locate emcmake or emcmake.bat" >&2
    exit 1
fi

mkdir -p "$BUILD_ROOT" "$LIBRARY_LOG_DIR"

LIBRARY_CONFIGURE_LOG="$LIBRARY_LOG_DIR/library-configure.log"
LIBRARY_BUILD_LOG="$LIBRARY_LOG_DIR/library-build.log"

echo "Building bindings library..."
if ! "$EMCMAKE_CMD" cmake -S "$BINDINGS_ROOT" -B "$LIBRARY_BUILD_DIR" >"$LIBRARY_CONFIGURE_LOG" 2>&1; then
    echo "Library configure failed. See $LIBRARY_CONFIGURE_LOG" >&2
    exit 1
fi

if ! cmake --build "$LIBRARY_BUILD_DIR" >"$LIBRARY_BUILD_LOG" 2>&1; then
    echo "Library build failed. See $LIBRARY_BUILD_LOG" >&2
    exit 1
fi

declare -a CASE_NAMES=(
    "ok_http_handlers_basic"
    "error_http_handler_no_args"
    "error_http_handler_immutable_ctx"
    "error_http_handler_wrong_ctx"
    "error_http_handler_no_request_arg"
    "error_http_handler_wrong_request_arg_type"
    "error_http_handler_no_return_type"
    "error_http_handler_wrong_return_type"
    "error_http_handler_no_sender"
    "error_http_handler_no_connection_id"
    "error_http_handler_no_db"
    "error_http_router_not_a_function"
    "error_http_router_with_args"
    "error_http_router_wrong_return_type"
)

declare -A CASE_EXPECTATION
declare -A CASE_MARKER
declare -A CASE_SOURCE

CASE_EXPECTATION["ok_http_handlers_basic"]="success"
CASE_SOURCE["ok_http_handlers_basic"]="$SCRIPT_DIR/cases/http-handlers/ok_http_handlers_basic.cpp"

CASE_EXPECTATION["error_http_handler_no_args"]="failure"
CASE_MARKER["error_http_handler_no_args"]="too few arguments provided to function-like macro invocation"
CASE_SOURCE["error_http_handler_no_args"]="$SCRIPT_DIR/cases/http-handlers/error_http_handler_no_args.cpp"

CASE_EXPECTATION["error_http_handler_immutable_ctx"]="failure"
CASE_MARKER["error_http_handler_immutable_ctx"]="First parameter of HTTP handler must be HandlerContext"
CASE_SOURCE["error_http_handler_immutable_ctx"]="$SCRIPT_DIR/cases/http-handlers/error_http_handler_immutable_ctx.cpp"

CASE_EXPECTATION["error_http_handler_wrong_ctx"]="failure"
CASE_MARKER["error_http_handler_wrong_ctx"]="First parameter of HTTP handler must be HandlerContext"
CASE_SOURCE["error_http_handler_wrong_ctx"]="$SCRIPT_DIR/cases/http-handlers/error_http_handler_wrong_ctx.cpp"

CASE_EXPECTATION["error_http_handler_no_request_arg"]="failure"
CASE_MARKER["error_http_handler_no_request_arg"]="too few arguments provided to function-like macro invocation"
CASE_SOURCE["error_http_handler_no_request_arg"]="$SCRIPT_DIR/cases/http-handlers/error_http_handler_no_request_arg.cpp"

CASE_EXPECTATION["error_http_handler_wrong_request_arg_type"]="failure"
CASE_MARKER["error_http_handler_wrong_request_arg_type"]="Second parameter of HTTP handler must be HttpRequest"
CASE_SOURCE["error_http_handler_wrong_request_arg_type"]="$SCRIPT_DIR/cases/http-handlers/error_http_handler_wrong_request_arg_type.cpp"

CASE_EXPECTATION["error_http_handler_no_return_type"]="failure"
CASE_MARKER["error_http_handler_no_return_type"]="non-void function does not return a value"
CASE_SOURCE["error_http_handler_no_return_type"]="$SCRIPT_DIR/cases/http-handlers/error_http_handler_no_return_type.cpp"

CASE_EXPECTATION["error_http_handler_wrong_return_type"]="failure"
CASE_MARKER["error_http_handler_wrong_return_type"]="no viable conversion from returned value of type 'unsigned int' to function return type 'SpacetimeDB::HttpResponse'"
CASE_SOURCE["error_http_handler_wrong_return_type"]="$SCRIPT_DIR/cases/http-handlers/error_http_handler_wrong_return_type.cpp"

CASE_EXPECTATION["error_http_handler_no_sender"]="failure"
CASE_MARKER["error_http_handler_no_sender"]="no member named 'sender' in 'SpacetimeDB::HandlerContext'"
CASE_SOURCE["error_http_handler_no_sender"]="$SCRIPT_DIR/cases/http-handlers/error_http_handler_no_sender.cpp"

CASE_EXPECTATION["error_http_handler_no_connection_id"]="failure"
CASE_MARKER["error_http_handler_no_connection_id"]="no member named 'connection_id' in 'SpacetimeDB::HandlerContext'"
CASE_SOURCE["error_http_handler_no_connection_id"]="$SCRIPT_DIR/cases/http-handlers/error_http_handler_no_connection_id.cpp"

CASE_EXPECTATION["error_http_handler_no_db"]="failure"
CASE_MARKER["error_http_handler_no_db"]="no member named 'db' in 'SpacetimeDB::HandlerContext'"
CASE_SOURCE["error_http_handler_no_db"]="$SCRIPT_DIR/cases/http-handlers/error_http_handler_no_db.cpp"

CASE_EXPECTATION["error_http_router_not_a_function"]="failure"
CASE_MARKER["error_http_router_not_a_function"]="illegal initializer"
CASE_SOURCE["error_http_router_not_a_function"]="$SCRIPT_DIR/cases/http-handlers/error_http_router_not_a_function.cpp"

CASE_EXPECTATION["error_http_router_with_args"]="failure"
CASE_MARKER["error_http_router_with_args"]="too many arguments provided to function-like macro invocation"
CASE_SOURCE["error_http_router_with_args"]="$SCRIPT_DIR/cases/http-handlers/error_http_router_with_args.cpp"

CASE_EXPECTATION["error_http_router_wrong_return_type"]="failure"
CASE_MARKER["error_http_router_wrong_return_type"]="no viable conversion from returned value of type 'unsigned int' to function return type 'SpacetimeDB::Router'"
CASE_SOURCE["error_http_router_wrong_return_type"]="$SCRIPT_DIR/cases/http-handlers/error_http_router_wrong_return_type.cpp"

FAILURES=0

for CASE_NAME in "${CASE_NAMES[@]}"; do
    CASE_BUILD_DIR="$BUILD_ROOT/$CASE_NAME"
    CONFIGURE_LOG="$CASE_BUILD_DIR/configure.log"
    BUILD_LOG="$CASE_BUILD_DIR/build.log"

    rm -rf "$CASE_BUILD_DIR"
    mkdir -p "$CASE_BUILD_DIR"
    cp "$TEMPLATE_PATH" "$CASE_BUILD_DIR/CMakeLists.txt"

    echo "Running $CASE_NAME..."

    CONFIGURE_EXIT=0
    BUILD_EXIT=0

    if "$EMCMAKE_CMD" cmake -S "$CASE_BUILD_DIR" -B "$CASE_BUILD_DIR" \
        -DMODULE_SOURCE="${CASE_SOURCE[$CASE_NAME]}" \
        -DOUTPUT_NAME="$CASE_NAME" \
        -DSPACETIMEDB_LIBRARY_DIR="$LIBRARY_BUILD_DIR" \
        -DSPACETIMEDB_INCLUDE_DIR="$INCLUDE_DIR" >"$CONFIGURE_LOG" 2>&1; then
        CONFIGURE_EXIT=0
    else
        CONFIGURE_EXIT=$?
    fi

    if [[ $CONFIGURE_EXIT -eq 0 ]]; then
        if cmake --build "$CASE_BUILD_DIR" >"$BUILD_LOG" 2>&1; then
            BUILD_EXIT=0
        else
            BUILD_EXIT=$?
        fi
    fi

    COMBINED_LOG=""
    [[ -f "$CONFIGURE_LOG" ]] && COMBINED_LOG+="$(cat "$CONFIGURE_LOG")"$'\n'
    [[ -f "$BUILD_LOG" ]] && COMBINED_LOG+="$(cat "$BUILD_LOG")"

    PASS=0
    DETAIL=""

    if [[ "${CASE_EXPECTATION[$CASE_NAME]}" == "success" ]]; then
        if [[ $CONFIGURE_EXIT -eq 0 && $BUILD_EXIT -eq 0 ]]; then
            PASS=1
        else
            DETAIL="Expected build success."
        fi
    else
        if [[ $CONFIGURE_EXIT -ne 0 || $BUILD_EXIT -ne 0 ]]; then
            if [[ "$COMBINED_LOG" == *"${CASE_MARKER[$CASE_NAME]}"* ]]; then
                PASS=1
            else
                DETAIL="Expected marker not found: ${CASE_MARKER[$CASE_NAME]}"
            fi
        else
            DETAIL="Expected build failure."
        fi
    fi

    if [[ $PASS -eq 1 ]]; then
        printf '%-40s PASS\n' "$CASE_NAME"
    else
        printf '%-40s FAIL\n' "$CASE_NAME"
        [[ -z "$DETAIL" ]] && DETAIL="$(printf '%s' "$COMBINED_LOG" | grep -v '^[[:space:]]*$' | head -n 8 | tr '\n' ' ')"
        echo "  $DETAIL"
        FAILURES=1
    fi
done

if [[ $FAILURES -ne 0 ]]; then
    echo
    echo "Compile test failures detected."
    exit 1
fi

echo
echo "All compile tests passed."
