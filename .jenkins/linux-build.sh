#!/bin/bash

set -euo pipefail

export GIT_SSH_COMMAND='ssh -i /home/jenkins/.ssh/id_spacetime'
export PATH="$PATH:/home/jenkins/.cargo/bin"

cd SpacetimeDB
git fetch -a origin
git checkout -f origin/live-cli
cargo build --release
