#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

DRY_RUN=0

if [ "$#" != "0" ]; then
    if [ "$1" != "--dry-run" ]; then
        echo "$1 is not a valid flag";
        exit 1;
    else
        DRY_RUN=1
    fi
fi

if [ $DRY_RUN != 1 ] ; then
	echo "You are about to publish to crates.io (dry run is false.)"
	echo "We are also going to do a test install after publishing. This will remove any version of spacetimedb-cli you may have installed."
	echo
	echo "Press [Enter] to continue."
	read -r
fi

BASEDIR=$(pwd)
declare -a CRATES=("metrics" "primitives" "bindings-macro" "bindings-sys" "sats" "lib" "bindings" "vm" "client-api-messages" "core" "client-api" "standalone" "cli" "sdk")

for crate in "${CRATES[@]}" ; do
	if [ ! -d "${BASEDIR}/crates/${crate}" ] ; then
		echo "This crate does not exist: ${crate}"
		exit 1
	fi
done

for crate in "${CRATES[@]}" ; do
	cd "${BASEDIR}/crates/${crate}"
	if [ $DRY_RUN == 1 ] ; then
		cargo publish --dry-run
	else
		cargo publish
	fi
done

echo "Doing a test install."

set +e
cargo uninstall spacetimedb-cli > /dev/null
set -e

echo
if cargo install spacetimedb-cli ; then
	echo "Installation was successful. Congrats on publishing to crates.io!"
else
	echo "ERROR: Installation failed! Check the build log for details. This typically means you forgot to update the version of a dependency."
fi
