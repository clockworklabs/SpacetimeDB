#!/bin/bash

set -euo pipefail

if [ $# != 1 ] ; then
	echo "Incorrect number of arguments. Expected: 1 Received: $#"
	exit 1
fi

fsed() {
	if [[ "$OSTYPE" == "darwin"* ]]; then
        sed -i.sed_bak "$@"
        rm -f "$2.sed_bak"
	else
        sed -i "$@"
	fi
}

version="$1"
declare -a crates=("bench" "bindings" "bindings-macro" "bindings-sys" "client-api-messages" "cli" "client-api" "core" "lib" "sats" "standalone" "testing")
upgrade_version() {
	toml=crates/$1/Cargo.toml
	if [ ! -f "$toml" ] ; then
		echo "Invalid crate: $1"
		exit 1
	fi

	# Upgrade the crate version
	if [[ $# -lt 2 ]] || [ "$2" != "--skip-version" ] ; then
		fsed '3s/.*version.*/version = "'"${version}"'"/' "${toml}"
	fi

	# Upgrade any dependencies
	for crate in "${crates[@]}" ; do
		if [[ $# -lt 2 ]] || [ "$2" != "--no-include-version" ] ; then
			fsed 's/.*'"spacetimedb-${crate}"'\s*=.*/'"spacetimedb-${crate}"' = { path = "..\/'"${crate}"'", version = "'"$version"'" }/' "${toml}"
		else
			fsed 's/.*'"spacetimedb-${crate}"'\s*=.*/'"spacetimedb-${crate}"' = { path = "..\/'"${crate}"'" }/' "${toml}"
		fi
	done
}

upgrade_version bench --no-include-version
upgrade_version client-api --no-include-version
upgrade_version core --no-include-version
upgrade_version standalone --no-include-version

upgrade_version bindings
upgrade_version bindings-macro
upgrade_version bindings-sys
upgrade_version cli
upgrade_version lib
upgrade_version sats
upgrade_version client-sdk

upgrade_version testing --skip-version

# Upgrade the template that is shipped with the cli
fsed 's@.*spacetimedb.*=.*".*".*@spacetimedb = "'"${version}"'"@' "crates/cli/src/subcommands/project/Cargo._toml"
fsed 's@.*spacetimedb-lib.*=.*@spacetimedb-lib = { path = "../lib", default-features = false }@' "crates/bindings/Cargo.toml"
fsed 's@.*spacetimedb-bindings-macro.*=.*@spacetimedb-bindings-macro = { path = "../bindings-macro" }@' "crates/bindings/Cargo.toml"

# Maintain any other options
fsed 's@.*spacetimedb-lib.*=.*@spacetimedb-lib = { path = "../lib", default-features = false, version = "'"$version"'"}@' "crates/bindings/Cargo.toml"
fsed 's@.*spacetimedb-bindings-macro.*=.*@spacetimedb-bindings-macro = { path = "../bindings-macro", version = "'"$version"'"}@' "crates/bindings/Cargo.toml"

cargo check

printf "Upgrade to version %s was successful.\n\n" "${version}"
