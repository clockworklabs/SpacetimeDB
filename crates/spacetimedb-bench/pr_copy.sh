#!/bin/bash
set -euo pipefail

if [ "$#" -lt "1" ] ; then
  echo "Usage: $0 <cmd>"
  exit 1
fi

cd "$(dirname "$0")"

TEMPD=$(mktemp -d)

# Exit if the temp directory wasn't created successfully.
if [ ! -e "$TEMPD" ]; then
    >&2 echo "Failed to create temp directory"
    exit 1
fi

CURRENT=$(pwd)

git clone ../../ $TEMPD
cd $TEMPD
git switch master
cp -r $CURRENT $TEMPD/crates/
cd $TEMPD/crates/spacetimedb-bench
$1
echo "Copy old.json..."
cp $TEMPD/crates/spacetimedb-bench/old.json $CURRENT
echo "Done"