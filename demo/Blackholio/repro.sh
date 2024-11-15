#!/bin/bash

set -ueo pipefail

SERVER=${1-local}

cd "$(readlink -f "$(dirname "$0")")"
cd server

#echo >/dev/stderr "Publishing.."
#spacetime publish -s "${SERVER}" circle-test --delete-data --yes

echo >/dev/stderr "Updating count.."
sed -i'' -e 's/\<600\>/6000/' src/lib.rs

echo >/dev/stderr "Republishing.."
if spacetime publish -s "${SERVER}" circle-test --build-options="--skip-println-checks"; then
  echo "---- THIS DID NOT FAIL!!!!! ----"
fi

echo >/dev/stderr "Reverting count.."
sed -i'' -e 's/\<6000\>/600/' src/lib.rs
