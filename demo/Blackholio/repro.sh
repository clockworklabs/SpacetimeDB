#!/bin/bash

set -u

cd "$(readlink -f "$(dirname "$0")")"
cd server

echo >/dev/stderr "Publishing.."
spacetime publish -s local circle-test --delete-data --yes

echo >/dev/stderr "Updating count.."
sed -i'' -e 's/\<600\>/6000/' src/lib.rs

echo >/dev/stderr "Republishing.."
spacetime publish -s local circle-test

echo >/dev/stderr "Reverting count.."
sed -i'' -e 's/\<6000\>/600/' src/lib.rs
