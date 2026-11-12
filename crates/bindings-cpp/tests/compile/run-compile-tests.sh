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

if [[ "$SUITE" != "http-handlers" && "$SUITE" != "indexes" && "$SUITE" != "idc" ]]; then
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

declare -A CASE_EXPECTATION
declare -A CASE_MARKER
declare -A CASE_SOURCE

if [[ "$SUITE" == "http-handlers" ]]; then
    declare -a CASE_NAMES=(
        "ok_http_handlers_basic"
        "error_http_handler_no_args"
        "error_http_handler_immutable_ctx"
        "error_http_handler_wrong_ctx"
        "error_http_handler_no_request_arg"
        "error_http_handler_wrong_request_arg_type"
        "error_http_handler_no_return_type"
        "error_http_handler_wrong_return_type"
        "error_http_handler_no_connection_id"
        "error_http_handler_no_db"
        "error_http_router_not_a_function"
        "error_http_router_with_args"
        "error_http_router_wrong_return_type"
    )

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
elif [[ "$SUITE" == "indexes" ]]; then
    declare -a CASE_NAMES=(
        "ok_multi_column_range_prefixes"
        "ok_autoinc_same_line"
        "error_multi_column_range_not_terminal"
    )

    CASE_EXPECTATION["ok_multi_column_range_prefixes"]="success"
    CASE_SOURCE["ok_multi_column_range_prefixes"]="$SCRIPT_DIR/cases/indexes/ok_multi_column_range_prefixes.cpp"

    CASE_EXPECTATION["ok_autoinc_same_line"]="success"
    CASE_SOURCE["ok_autoinc_same_line"]="$SCRIPT_DIR/cases/indexes/ok_autoinc_same_line.cpp"

    CASE_EXPECTATION["error_multi_column_range_not_terminal"]="failure"
    CASE_MARKER["error_multi_column_range_not_terminal"]="Range<T> in a multi-column index filter must be the final supplied element"
    CASE_SOURCE["error_multi_column_range_not_terminal"]="$SCRIPT_DIR/cases/indexes/error_multi_column_range_not_terminal.cpp"
else
    # Receiver bindings come from crates/bindings-cpp/tests/fixtures/idc_receiver,
    # which the codegen tests keep in sync with the generator.
    declare -a CASE_NAMES=(
        "ok_receiver_bindings"
        "error_identity_from_other_dependency"
        "error_identity_implicit_conversion"
        "ok_manual_outbox"
        "error_outbox_delete"
        "error_outbox_update_by_key"
        "error_outbox_target_wrong_type"
        "error_outbox_target_on_regular_table"
        "error_outbox_param_unknown"
        "error_outbox_param_wrong_type"
        "error_outbox_receiver_not_handle"
        "error_on_result_wrong_row"
        "error_on_result_wrong_result"
        "error_on_result_regular_table"
    )

    CASE_EXPECTATION["ok_receiver_bindings"]="success"
    CASE_SOURCE["ok_receiver_bindings"]="$SCRIPT_DIR/cases/idc/ok_receiver_bindings.cpp"

    CASE_EXPECTATION["error_identity_from_other_dependency"]="failure"
    CASE_MARKER["error_identity_from_other_dependency"]="no viable conversion from 'RemoteIdentity<other::detail::module_tag>' to 'RemoteIdentity<receiver::detail::module_tag>'"
    CASE_SOURCE["error_identity_from_other_dependency"]="$SCRIPT_DIR/cases/idc/error_identity_from_other_dependency.cpp"

    CASE_EXPECTATION["error_identity_implicit_conversion"]="failure"
    CASE_MARKER["error_identity_implicit_conversion"]="no viable conversion from 'Identity' to 'receiver::Identity'"
    CASE_SOURCE["error_identity_implicit_conversion"]="$SCRIPT_DIR/cases/idc/error_identity_implicit_conversion.cpp"

    CASE_EXPECTATION["ok_manual_outbox"]="success"
    CASE_SOURCE["ok_manual_outbox"]="$SCRIPT_DIR/cases/idc/ok_manual_outbox.cpp"

    CASE_EXPECTATION["error_outbox_delete"]="failure"
    CASE_MARKER["error_outbox_delete"]="attempt to use a deleted function"
    CASE_SOURCE["error_outbox_delete"]="$SCRIPT_DIR/cases/idc/error_outbox_delete.cpp"

    CASE_EXPECTATION["error_outbox_update_by_key"]="failure"
    CASE_MARKER["error_outbox_update_by_key"]="attempt to use a deleted function"
    CASE_SOURCE["error_outbox_update_by_key"]="$SCRIPT_DIR/cases/idc/error_outbox_update_by_key.cpp"

    CASE_EXPECTATION["error_outbox_target_wrong_type"]="failure"
    CASE_MARKER["error_outbox_target_wrong_type"]="The outbox target column 'target' must have the receiver bindings' Identity type."
    CASE_SOURCE["error_outbox_target_wrong_type"]="$SCRIPT_DIR/cases/idc/error_outbox_target_wrong_type.cpp"

    CASE_EXPECTATION["error_outbox_target_on_regular_table"]="failure"
    CASE_MARKER["error_outbox_target_on_regular_table"]="FIELD_Target applies only to outbox tables declared with SPACETIMEDB_OUTBOX_TABLE."
    CASE_SOURCE["error_outbox_target_on_regular_table"]="$SCRIPT_DIR/cases/idc/error_outbox_target_on_regular_table.cpp"

    CASE_EXPECTATION["error_outbox_param_unknown"]="failure"
    CASE_MARKER["error_outbox_param_unknown"]="the receiver reducer has no parameter named 'text'."
    CASE_SOURCE["error_outbox_param_unknown"]="$SCRIPT_DIR/cases/idc/error_outbox_param_unknown.cpp"

    CASE_EXPECTATION["error_outbox_param_wrong_type"]="failure"
    CASE_MARKER["error_outbox_param_wrong_type"]="the column's type doesn't match the receiver parameter's type."
    CASE_SOURCE["error_outbox_param_wrong_type"]="$SCRIPT_DIR/cases/idc/error_outbox_param_wrong_type.cpp"

    CASE_EXPECTATION["error_outbox_receiver_not_handle"]="failure"
    CASE_MARKER["error_outbox_receiver_not_handle"]="The outbox receiver must be a reducer handle from generated receiver bindings"
    CASE_SOURCE["error_outbox_receiver_not_handle"]="$SCRIPT_DIR/cases/idc/error_outbox_receiver_not_handle.cpp"

    CASE_EXPECTATION["error_on_result_wrong_row"]="failure"
    CASE_MARKER["error_on_result_wrong_row"]="The second parameter of an on_result reducer must be the outbox table's row type."
    CASE_SOURCE["error_on_result_wrong_row"]="$SCRIPT_DIR/cases/idc/error_on_result_wrong_row.cpp"

    CASE_EXPECTATION["error_on_result_wrong_result"]="failure"
    CASE_MARKER["error_on_result_wrong_result"]="The third parameter of an on_result reducer must be SpacetimeDB::OutboxResult."
    CASE_SOURCE["error_on_result_wrong_result"]="$SCRIPT_DIR/cases/idc/error_on_result_wrong_result.cpp"

    CASE_EXPECTATION["error_on_result_regular_table"]="failure"
    CASE_MARKER["error_on_result_regular_table"]="SPACETIMEDB_REDUCER_ON_RESULT requires an outbox table declared with SPACETIMEDB_OUTBOX_TABLE."
    CASE_SOURCE["error_on_result_regular_table"]="$SCRIPT_DIR/cases/idc/error_on_result_regular_table.cpp"
fi

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
        -DEXTRA_INCLUDE_DIR="$BINDINGS_ROOT/tests/fixtures/idc_receiver" \
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
