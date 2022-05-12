#!/bin/bash

rustup install nightly
rustup component add rustfmt --toolchain nightly

cd "$(dirname "$0")"

cp -v pre-commit ../.git/hooks
