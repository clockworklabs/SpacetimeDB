#!/usr/bin/env bash

# This script requires a running local SpacetimeDB instance.

set -ueo pipefail

SDK_PATH="$(dirname "$0")/.."
SDK_PATH="$(realpath "$SDK_PATH")"
STDB_PATH="$SDK_PATH/../.."
SPACETIMEDB_SERVER_URL="${SPACETIMEDB_SERVER_URL:-local}"

DOTNET_VERSIONS=("$@")
if [ ${#DOTNET_VERSIONS[@]} -eq 0 ]; then
    DOTNET_VERSIONS=(8 10)
fi

GLOBAL_JSON_BACKUPS=()

expected_global_json_symlink_target() {
    local path="$1"

    case "$path" in
        "$SDK_PATH/examples~/regression-tests/server/global.json") echo "../../../../../global.json" ;;
        "$SDK_PATH/examples~/regression-tests/republishing/server-initial/global.json") echo "../../../../../../global.json" ;;
        "$SDK_PATH/examples~/regression-tests/republishing/server-republish/global.json") echo "../../../../../../global.json" ;;
        *) return 1 ;;
    esac
}

backup_global_json_once() {
    local path="$1"
    local entry
    for entry in "${GLOBAL_JSON_BACKUPS[@]}"; do
        if [[ "$entry" == "$path|"* ]]; then
            return
        fi
    done

    if [ -L "$path" ]; then
        GLOBAL_JSON_BACKUPS+=("$path|symlink:$(readlink "$path")")
    elif [ -e "$path" ]; then
        local backup
        backup="$(mktemp)"
        cp "$path" "$backup"
        GLOBAL_JSON_BACKUPS+=("$path|file:$backup")
    elif target="$(expected_global_json_symlink_target "$path")"; then
        GLOBAL_JSON_BACKUPS+=("$path|symlink:$target")
    else
        GLOBAL_JSON_BACKUPS+=("$path|missing:")
    fi
}

restore_global_jsons() {
    local entry path kind
    for entry in "${GLOBAL_JSON_BACKUPS[@]}"; do
        path="${entry%%|*}"
        kind="${entry#*|}"
        rm -f "$path"
        case "$kind" in
            symlink:*) ln -s "${kind#symlink:}" "$path" ;;
            file:*) mv "${kind#file:}" "$path" ;;
            missing:) ;;
        esac
    done
}

write_dotnet_global_json() {
    local dir="$1"
    local version="$2"
    local path="$dir/global.json"

    backup_global_json_once "$path"
    rm -f "$path"

    case "$version" in
        8) echo '{"sdk":{"version":"8.0.100","rollForward":"latestFeature"}}' > "$path" ;;
        10) echo '{"sdk":{"version":"10.0.100","rollForward":"latestMinor"}}' > "$path" ;;
        *) echo "Unsupported .NET version: $version" >&2; exit 1 ;;
    esac
}

configure_csharp_modules_sdk() {
    local dotnet_version="$1"
    write_dotnet_global_json "$SDK_PATH/examples~/regression-tests/server" "$dotnet_version"
    write_dotnet_global_json "$SDK_PATH/examples~/regression-tests/republishing/server-initial" "$dotnet_version"
    write_dotnet_global_json "$SDK_PATH/examples~/regression-tests/republishing/server-republish" "$dotnet_version"
    write_dotnet_global_json "$STDB_PATH/modules/sdk-test-procedure" "$dotnet_version"
}

run_client() {
    local dir="$1"
    local dotnet_version="$2"
    if [ "$dotnet_version" = "10" ]; then
        (cd "$dir" && EXPERIMENTAL_WASM_AOT=1 dotnet run -c Debug)
    else
        (cd "$dir" && env -u EXPERIMENTAL_WASM_AOT dotnet run -c Debug)
    fi
}

trap restore_global_jsons EXIT

# Build and run SpacetimeDB server
cargo build --manifest-path "$STDB_PATH/crates/standalone/Cargo.toml"

for dotnet_version in "${DOTNET_VERSIONS[@]}"; do
    echo "Running C# regression tests with .NET $dotnet_version"

    configure_csharp_modules_sdk "$dotnet_version"

    # Regenerate bindings from the C# modules with the same SDK version used for publish.
    "$SDK_PATH/tools~/gen-regression-tests.sh" "$dotnet_version"

    # Publish module for btree test
    cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- publish --dotnet-version "$dotnet_version" -c -y --server "$SPACETIMEDB_SERVER_URL" -p "$SDK_PATH/examples~/regression-tests/server" btree-repro

    # Publish module for republishing module test
    cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- publish --dotnet-version "$dotnet_version" -c -y --server "$SPACETIMEDB_SERVER_URL" -p "$SDK_PATH/examples~/regression-tests/republishing/server-initial" republish-test
    cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" call --server "$SPACETIMEDB_SERVER_URL" republish-test insert 1
    cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- publish --dotnet-version "$dotnet_version" --server "$SPACETIMEDB_SERVER_URL" -p "$SDK_PATH/examples~/regression-tests/republishing/server-republish" --break-clients republish-test
    cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" call --server "$SPACETIMEDB_SERVER_URL" republish-test insert 2

    echo "Cleanup obj~ folders generated in $SDK_PATH/examples~/regression-tests/procedure-client"
    # There is a bug in the code generator that creates obj~ folders in the output directory using a Rust project.
    rm -rf "$SDK_PATH/examples~/regression-tests/procedure-client"/*/obj~
    rm -rf "$SDK_PATH/examples~/regression-tests/procedure-client/module_bindings"/*/obj~

    # Publish module for procedure tests
    cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- publish --dotnet-version "$dotnet_version" -c -y --server "$SPACETIMEDB_SERVER_URL" -p "$STDB_PATH/modules/sdk-test-procedure" procedure-tests

    # Run clients against the modules published with this .NET version.
    run_client "$SDK_PATH/examples~/regression-tests/client" "$dotnet_version"
    run_client "$SDK_PATH/examples~/regression-tests/republishing/client" "$dotnet_version"
    run_client "$SDK_PATH/examples~/regression-tests/procedure-client" "$dotnet_version"
done
