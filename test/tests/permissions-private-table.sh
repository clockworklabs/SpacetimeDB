#!/bin/bash

if [ "$DESCRIBE_TEST" = 1 ] ; then
    echo "Tests that a private table can only be queried by the database owner"
    exit
fi

set -eoux pipefail

source "./test/lib.include"

cat > "${PROJECT_PATH}/src/lib.rs" <<EOF
use spacetimedb::spacetimedb;

#[spacetimedb(table)]
#[sats(name = "_Secret")]
pub struct Secret {
    answer: u8,
}

#[spacetimedb(init)]
pub fn init() {
    Secret::insert(Secret { answer: 42 });
}
EOF

run_test cargo run publish --skip_clippy --project-path "$PROJECT_PATH"
DATABASE="$(grep "Created new database" "$TEST_OUT" | awk 'NF>1{print $NF}')"

run_test cargo run sql "$DATABASE" 'select * from _Secret'
result="$(tail -n 3 "$TEST_OUT")"
[ "${result//[$'\n\r\t ']}" == "answer--------42" ]

reset_config
run_test cargo run identity new --no-email
IDENT="$(grep IDENTITY "$TEST_OUT" | awk '{print $2}')"
run_test cargo run identity set-default "$IDENT"

if cargo run sql "$DATABASE" 'select * from _Secret'; then exit 1; fi
