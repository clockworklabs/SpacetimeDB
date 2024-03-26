#!/bin/bash

# color constants
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

function assert_dir {
    if [ ! -d $1 ]; then
        echo -e "${RED}$1 does not exist${NC}"
        exit 1
    fi
}
function assert_file {
    if [ ! -f $1 ]; then
        echo -e "${RED}$1 does not exist${NC}"
        exit 1
    fi
}
function cleanup {
    kill $SPACETIME_PROCESS
    kill $TRACY_PROCESS
}

BOTSDIR="$HOME/bots"
OUTDIR="$PWD/bottest"
TRACY_CAPTURE_BIN="/home/ubuntu/tracy/capture/build/unix/capture-release"

echo "Expecting to find an unzipped bots directory at $BOTSDIR"
assert_dir "$BOTSDIR"
assert_file "$BOTSDIR/bitcraft_spacetimedb_with_wasm_opt.wasm"
assert_file "$BOTSDIR/bitcraft-bots.tar"
assert_file "$BOTSDIR/deploy_world.py"
assert_file "$BOTSDIR/docker-compose.yml"
echo "Expecting to find tracy capture executable at $TRACY_CAPTURE_BIN (tracy should be on git tag v0.10)"
assert_file "$TRACY_CAPTURE_BIN"

set -exo pipefail

docker --version
docker-compose --version
rustc --version

trap cleanup EXIT

# move into containing folder, keep paths stable
cd "$(dirname "$0")"

mkdir -p bottest 
cargo build --manifest-path ../../Cargo.toml --bin spacetime --release
../../target/release/spacetime start --listen-addr 0.0.0.0:3000 --enable-tracy 2>&1 >$OUTDIR/spacetime.log &
SPACETIME_PROCESS=$!
sleep 5
$TRACY_CAPTURE_BIN -o $OUTDIR/output.tracy 2>&1 >$OUTDIR/tracy-capture.log&
TRACY_PROCESS=$!
../../target/release/spacetime publish -c bitcraft --wasm-file "$BOTSDIR/bitcraft_spacetimedb_with_wasm_opt.wasm"
python3 "$BOTSDIR/deploy_world.py" -H http://127.0.0.1:3000 -m bitcraft -f "$BOTSDIR/Spacetime128x128.snapshot"
docker load --input "$BOTSDIR/bitcraft-bots.tar"
REPLICAS=30 docker-compose -f "$BOTSDIR/docker-compose.yml" up &
sleep 60
docker-compose -f "$BOTSDIR/docker-compose.yml" down

