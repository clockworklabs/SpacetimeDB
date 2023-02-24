#!/bin/bash

set -euo pipefail

if [ $# != 1 ] ; then
	echo "Incorrect number of arguments. Expected: 1 Received: $#"
	exit 1
fi

version="$1"
declare -a crates=("bindings" "bindings-macro" "bindings-sys" "cli" "lib" "client-api" "core" "standalone" "cloud" "bench")

upgrade_version() {
	toml=crates/$1/Cargo.toml
	if [ ! -f "$toml" ] ; then
		echo "Invalid crate: $1"
		exit 1
	fi

	# Upgrade the crate version
	sed -i '3s/.*version.*/version = "'"${version}"'"/' "${toml}"

	# Upgrade any dependencies
	for crate in "${crates[@]}" ; do
		sed -i 's/.*'"spacetimedb-${crate}"'\s*=.*/'"spacetimedb-${crate}"' = { path = "..\/'"${crate}"'" }/' "${toml}"
	done
}

for crate in "${crates[@]}" ; do
	upgrade_version "${crate}"
done

# Upgrade the template that is shipped with the cli
sed -i 's@.*spacetimedb.*=.*".*".*@spacetimedb = "'"${version}"'"@' "crates/cli/src/subcommands/project/Cargo._toml"
sed -i 's@.*spacetimedb-lib.*=.*@spacetimedb-lib = { path = "../lib", default-features = false }@' "crates/bindings/Cargo.toml"
sed -i 's@.*spacetimedb-bindings-macro.*=.*@spacetimedb-bindings-macro = { path = "../bindings-macro", optional = true }@' "crates/bindings/Cargo.toml"

cargo check

printf "Upgrade to version %s was successful.\n\n" "${version}"
