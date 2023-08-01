#!/bin/bash

rustup component add rustfmt

cd "$(dirname "$0")"

rm -rf ../.git/hooks
# Soft link the .git/hooks directory onto hooks/
ln -s ../hooks/hooks ../.git/hooks
