#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

DRY_RUN=0
ALLOW_DIRTY=0
SKIP_ALREADY_PUBLISHED=0
NEW_CRATE_OWNERS=("tyler@clockworklabs.io" "zeke@clockworklabs.io")

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run)
            DRY_RUN=1
            ;;
        --allow-dirty)
            ALLOW_DIRTY=1
            ;;
        --skip-already-published)
            SKIP_ALREADY_PUBLISHED=1
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

    # Check if crate exists on crates.io
    if ! cargo search "$crate" | grep -q "^$crate ="; then
        IS_NEW_CRATE=1
        echo "INFO: Detected $crate as a new crate on crates.io!"
    else
        IS_NEW_CRATE=0
    fi

    echo "Publishing crate: $crate with command: $PUBLISH_CMD"
    if ! OUTPUT=$($PUBLISH_CMD 2>&1); then
        if [ $SKIP_ALREADY_PUBLISHED -eq 1 ] && echo "$OUTPUT" | grep -q "crate version .* is already uploaded"; then
            echo "WARNING: Crate $crate version is already published. Skipping..."
        else
            echo "ERROR: Failed to publish $crate. Check logs:"
            echo "$OUTPUT"
            exit 1
        fi
    else
        # If this is a new crate, add owners
        if [ $IS_NEW_CRATE -eq 1 ]; then
            echo "INFO: Adding owners for new crate $crate..."
            for owner in "${NEW_CRATE_OWNERS[@]}"; do
                cargo owner --add "$owner"
                echo "INFO: Added $owner as an owner of $crate."
            done
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
