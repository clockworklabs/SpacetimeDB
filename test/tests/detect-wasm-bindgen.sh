#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
	echo "This tests uploading a basic module and calling some functions and checking logs afterwards."
        exit
fi

set -euox pipefail

source "./test/lib.include"

create_project

cat > "${PROJECT_PATH}/src/lib.rs" << EOF
use spacetimedb::{log, spacetimedb};

#[spacetimedb(reducer)]
pub fn test() {
    log::info!("Hello! {}", now());
}

#[wasm_bindgen::prelude::wasm_bindgen]
extern "C" {
    fn now() -> i32;
}
EOF

printf '\nwasm-bindgen = "0.2"\n' >> "${PROJECT_PATH}/Cargo.toml"

run_fail_test spacetime -p spacetimedb-cli -- build "${PROJECT_PATH}"

[ $(grep "wasm-bindgen detected" "$TEST_OUT" | wc -l ) == 1 ]

