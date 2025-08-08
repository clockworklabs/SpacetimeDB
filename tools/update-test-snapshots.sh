#!/bin/bash

set -euo pipefail

cd "$(dirname $0)/.."

function cmd_exists() {
	command -v $1 > /dev/null || return 1
}

if ! cmd_exists cargo-insta; then
  echo "The cargo-insta command is not installed."
  read -p 'Would you like to install it with `cargo install`? [Y/n] ' resp
  case ${resp,,} in
    ''|y|yes) cargo install cargo-insta ;;
    n|no)
      if ! cmd_exists cargo-insta; then
	echo "cargo-insta not installed; aborting"
	exit 1
      fi
      ;;
    *) echo "unknown option $resp"; exit 1 ;;
  esac
fi

cargo build -p module-test --release --target wasm32-unknown-unknown

cargo insta test --review -p spacetimedb-cli
