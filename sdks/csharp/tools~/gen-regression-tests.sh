#!/usr/bin/env bash

set -ueo pipefail

SDK_PATH="$(dirname "$0")/.."
SDK_PATH="$(realpath "$SDK_PATH")"
STDB_PATH="$SDK_PATH/../.."
DOTNET_VERSION="${1:-}"

GLOBAL_JSON_BACKUPS=()

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

BUILD_OPTIONS=()
if [ -n "$DOTNET_VERSION" ]; then
    trap restore_global_jsons EXIT
    write_dotnet_global_json "$SDK_PATH/examples~/regression-tests/server" "$DOTNET_VERSION"
    write_dotnet_global_json "$SDK_PATH/examples~/regression-tests/republishing/server-republish" "$DOTNET_VERSION"
    write_dotnet_global_json "$STDB_PATH/modules/sdk-test-procedure" "$DOTNET_VERSION"
    BUILD_OPTIONS+=("--build-options=--dotnet-version $DOTNET_VERSION")
fi

cargo build --manifest-path "$STDB_PATH/crates/standalone/Cargo.toml"
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- generate -y -l csharp -o "$SDK_PATH/examples~/regression-tests/client/module_bindings" --module-path "$SDK_PATH/examples~/regression-tests/server" "${BUILD_OPTIONS[@]}"
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- generate -y -l csharp -o "$SDK_PATH/examples~/regression-tests/republishing/client/module_bindings" --module-path "$SDK_PATH/examples~/regression-tests/republishing/server-republish" "${BUILD_OPTIONS[@]}"
cargo run --manifest-path "$STDB_PATH/crates/cli/Cargo.toml" -- generate -y -l csharp -o "$SDK_PATH/examples~/regression-tests/procedure-client/module_bindings" --module-path "$STDB_PATH/modules/sdk-test-procedure" "${BUILD_OPTIONS[@]}"
