#!/bin/bash

cd "$(dirname "$0")"

cargo expand > ../rust-wasm-output/src/lib.rs
