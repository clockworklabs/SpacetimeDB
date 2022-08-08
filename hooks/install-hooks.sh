#!/bin/bash

rustup component add rustfmt

cd "$(dirname "$0")"

cp -v pre-commit ../.git/hooks
