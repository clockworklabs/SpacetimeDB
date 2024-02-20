#!/bin/bash

# script to enter iai dockerfile locally

set -exo pipefail

SCRIPT_DIR="$(dirname "$(readlink -f "$0")")"
cd "$SCRIPT_DIR"
docker build . --tag rust-iai-callgrind:latest 
docker run --privileged -v "$(realpath $PWD/../..):/projects/SpacetimeDB" -w /projects/SpacetimeDB/crates/bench rust-iai-callgrind:latest cargo bench --bench callgrind