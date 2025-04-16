#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

DRY_RUN=0
ALLOW_DIRTY=0
SKIP_ALREADY_PUBLISHED=0
# Use usernames here to help prevent users from getting spam
NEW_CRATE_OWNERS=("cloutiertyler" "bfops" "jdetter")

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
            echo "Skipping already published crates."
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
declare -a ROOTS=(bindings sdk cli standalone)
declare -a CRATES=($(python3 tools/find-publish-list.py --recursive --quiet "${ROOTS[@]}"))

echo Crates to publish: "${CRATES[@]}"
echo

for crate in "${CRATES[@]}"; do
    if [ ! -d "${BASEDIR}/crates/${crate}" ]; then
        echo "This crate does not exist: ${crate}"
        exit 1
    fi
done

i=0
for crate in "${CRATES[@]}"; do
    i=$(($i+1))
    cd "${BASEDIR}/crates/${crate}"

    PUBLISH_CMD="cargo publish"
    [[ $DRY_RUN -eq 1 ]] && PUBLISH_CMD+=" --dry-run"
    [[ $ALLOW_DIRTY -eq 1 ]] && PUBLISH_CMD+=" --allow-dirty"

    echo "[$i/${#CRATES[@]}] Publishing crate: $crate with command: $PUBLISH_CMD"
    if ! OUTPUT=$($PUBLISH_CMD 2>&1); then
        if [ $SKIP_ALREADY_PUBLISHED -eq 1 ] && echo "$OUTPUT" | grep -q "already exists"; then
            echo "WARNING: Crate $crate version is already published. Skipping..."
        else
            echo "ERROR: Failed to publish $crate. Check logs:"
            echo "$OUTPUT"
            exit 1
        fi
    fi

    # Add owners
    echo "INFO: Adding owners for $crate..."
    for owner in "${NEW_CRATE_OWNERS[@]}"; do
        if ! OUTPUT=$(cargo owner --add "$owner" 2>&1); then
          if echo "$OUTPUT" | grep -q "already" ; then
            echo "$owner already is an owner of the crate."
          else
            echo "Unknown error adding owner $owner:"
            echo "$OUTPUT"
            exit 1
          fi
        else
          echo "INFO: Added $owner as an owner of $crate."
        fi
    done
    echo
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
