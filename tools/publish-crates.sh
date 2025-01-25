#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

DRY_RUN=0
ALLOW_DIRTY=0

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            ;;
        --allow-dirty)
            ALLOW_DIRTY=1
            ;;
        *)
            echo "Invalid argument: $1"
            exit 1
            ;;
    esac
    shift
done

if [ $DRY_RUN -ne 1 ]; then
    echo "You are about to publish to crates.io (dry run is false.)"
    echo "We are also going to do a test install after publishing. This will remove any version of spacetimedb-cli you may have installed."
    echo
    echo "Press [Enter] to continue."
    read -r
fi

BASEDIR=$(pwd)

declare -a CRATES=("metrics" "primitives" "sql-parser" "bindings-macro" "bindings-sys" "data-structures" "sats" "lib" "schema" "bindings" "table" "vm" "client-api-messages" "paths" "commitlog" "durability" "fs-utils" "snapshot" "expr" "execution" "physical-plan" "query" "core" "client-api" "standalone" "cli" "sdk")

for crate in "${CRATES[@]}"; do
    if [ ! -d "${BASEDIR}/crates/${crate}" ]; then
        echo "This crate does not exist: ${crate}"
        exit 1
    fi
done

for crate in "${CRATES[@]}"; do
    cd "${BASEDIR}/crates/${crate}"

    PUBLISH_CMD="cargo publish"
    [[ $DRY_RUN -eq 1 ]] && PUBLISH_CMD+=" --dry-run"
    [[ $ALLOW_DIRTY -eq 1 ]] && PUBLISH_CMD+=" --allow-dirty"

    echo "Publishing crate: $crate with command: $PUBLISH_CMD"
    if ! OUTPUT=$($PUBLISH_CMD 2>&1); then
        if echo "$OUTPUT" | grep -q "crate version .* is already uploaded"; then
            echo "WARNING: Crate $crate version is already published. Skipping..."
        else
            echo "ERROR: Failed to publish $crate. Check logs:"
            echo "$OUTPUT"
            exit 1
        fi
    fi
done

echo "Doing a test install."

set +e
cargo uninstall spacetimedb-cli > /dev/null
set -e

echo
if cargo install spacetimedb-cli; then
    echo "Installation was successful. Congrats on publishing to crates.io!"
else
    echo "ERROR: Installation failed! Check the build log for details. This typically means you forgot to update the version of a dependency."
fi
