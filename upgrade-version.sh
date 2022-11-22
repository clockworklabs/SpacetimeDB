#!/bin/bash

set -euo pipefail

if [ $# != 1 ] ; then
	echo "Incorrect number of arguments. Expected: 1 Received: $#"
	exit 1
fi

version="$1"
declare -a crates=("spacetimedb-bindings" "spacetimedb-bindings-macro" "spacetimedb-bindings-sys" "spacetimedb-cli" "spacetimedb-lib")

upgrade_version() {
	toml=crates/$1/Cargo.toml

	# Upgrade the crate version
	sed -i '3s/.*version.*/version = "'"${version}"'"/' "${toml}"

	# Upgrade any dependencies
	for crate in "${crates[@]}" ; do
		if [ "${crate}" = "spacetimedb-bindings-macro" ] ; then
			sed -i 's/.*'"${crate}"'\s*=.*/'"${crate}"' = { path = "..\/'"${crate}"'", version = "'"${version}"'", optional = true }/' "${toml}"
		else

			sed -i 's/.*'"${crate}"'\s*=.*/'"${crate}"' = { path = "..\/'"${crate}"'", version = "'"${version}"'" }/' "${toml}"
		fi
	done
}

for crate in "${crates[@]}" ; do
	upgrade_version "${crate}"
done

# Upgrade the template that is shipped with spacetimedb-cli
sed -i 's/.*spacetimedb.*=.*".*".*/spacetimedb = "'"${version}"'"/' "crates/spacetimedb-cli/src/subcommands/project/Cargo._toml"

cargo check

printf "Upgrade to version %s was successful.\n\n" "${version}"
