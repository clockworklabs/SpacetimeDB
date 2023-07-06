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
FIRST_CRATE=1
declare -a CRATES=("bindings-macro" "bindings-sys" "sats" "lib" "bindings" "cli" "client-api-messages" "sdk")

for crate in "${CRATES[@]}" ; do
	if [ ! -d "${BASEDIR}/crates/${crate}" ] ; then
		echo "This crate does not exist: ${crate}"
		exit 1
	fi
done

for crate in "${CRATES[@]}" ; do
	if [ $FIRST_CRATE != 1 ] ; then
		echo "Waiting 60 seconds for crates.io to update..."
		sleep 60
	fi

	cd "${BASEDIR}/crates/${crate}"
	if [ $DRY_RUN == 1 ] ; then
		cargo publish --dry-run
	else
		cargo publish
	fi
	FIRST_CRATE=0
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
