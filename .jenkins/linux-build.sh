#!/bin/bash

set -euo pipefail

export GIT_SSH_COMMAND='ssh -i /home/jenkins/.ssh/id_spacetime'
export PATH="$PATH:/home/jenkins/.cargo/bin"

if [ $# != 1 ] ; then
	echo "A branch name is required as an argument."
	exit 1
fi

cd SpacetimeDB
git fetch -a origin
git checkout -f "origin/$1"
cargo build --release
