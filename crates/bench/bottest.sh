#!/bin/bash

# script to run a bot test. This is its own script for 2 reasons:
# 1. you can run it off CI
# 2. github actions can't run things in parallel anyway, so we need a shell script to run
#    the spacetime server, tracy capture, and the bots in parallel.

# move into project root, keep paths stable
cd "$(dirname "$0")/../.."

# color constants
RED='\033[0;31m'
NC='\033[0m'

function assert_dir {
    if [ ! -d "$1" ]; then
        echo -e "${RED}$1 does not exist${NC}"
        exit 1
    fi
}
function assert_file {
    if [ ! -f "$1" ]; then
        echo -e "${RED}$1 does not exist${NC}"
        exit 1
    fi
}
# we need to ensure everything dies in case of an early exit to avoid polluting
# the bot test machine with processes
function cleanup {
    kill $SPACETIME_PROCESS || echo "already dead"
    kill $TRACY_PROCESS || echo "already dead"
    git checkout -- .
}

BOTS_DIR="${BOTS_DIR:-$HOME/bots}"
TRACY_CAPTURE_BIN="${TRACY_CAPTURE_BIN:-$HOME/tracy/capture/build/unix/capture-release}"
OUT_DIR="$PWD/crates/bench/bottest"

echo "Expecting to find an unzipped bots directory at $BOTS_DIR (set BOTS_DIR env var to change)"
echo "(Ask John for a copy of the bots directory if you need one)"
assert_dir "$BOTS_DIR"
assert_file "$BOTS_DIR/bitcraft_spacetimedb_with_wasm_opt.wasm"
assert_file "$BOTS_DIR/bitcraft-bots.tar"
assert_file "$BOTS_DIR/deploy_world.py"
assert_file "$BOTS_DIR/docker-compose.yml"
echo "Expecting to find tracy capture executable $TRACY_CAPTURE_BIN (set TRACY_CAPTURE_BIN env var to change) (tracy should be on git tag v0.10)"
assert_file "$TRACY_CAPTURE_BIN"
if [ -z "$(git status --porcelain)" ]; then 
    # Working directory clean
    echo "Git is clean, can apply patch"
else 
    echo -e "${RED}Git has changes, cannot apply bot test patch. Please commit or stash your uncommitted changes${NC}"
    exit 1
fi

set -exo pipefail

echo ------- PRELIMINARIES -------

docker --version
docker-compose --version
rustc --version

# run cleanup when script terminates
trap cleanup EXIT

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/.spacetime"

# load bot docker image	
docker load --input "$BOTS_DIR/bitcraft-bots.tar"

git apply << EOF
diff --git a/Cargo.toml b/Cargo.toml
index 7caba831..8863989b 100644
--- a/Cargo.toml
+++ b/Cargo.toml
@@ -210,7 +210,7 @@ tokio-tungstenite = { version = "0.21", features = ["native-tls"] }
 tokio-util = { version = "0.7.4", features = ["time"] }
 toml = "0.8"
 tower-http = { version = "0.5", features = ["cors"] }
-tracing = { version = "0.1.37", features = ["release_max_level_off"] }
+tracing = { version = "0.1.37" } #, features = ["release_max_level_off"] }
 tracing-appender = "0.2.2"
 tracing-core = "0.1.31"
 tracing-flame = "0.2.0"
EOF

cargo build --bin spacetime --release

echo ------- PREPARING WORLD -------

target/release/spacetime start --listen-addr 0.0.0.0:3000 "$OUT_DIR/.spacetime" >"$OUT_DIR/spacetime_deploy_world.log" 2>&1 &
SPACETIME_PROCESS=$!
sleep 5

target/release/spacetime publish -c bitcraft --wasm-file "$BOTS_DIR/bitcraft_spacetimedb_with_wasm_opt.wasm"

python3 "$BOTS_DIR/deploy_world.py" -H http://127.0.0.1:3000 -m bitcraft -f "$BOTS_DIR/Spacetime128x128.snapshot" >"$OUT_DIR/deploy_world.log" 2>&1

echo ------- RUNNING BOTS AND COLLECTING TRACE -------

kill -INT "$SPACETIME_PROCESS"
sleep 5

target/release/spacetime start --listen-addr 0.0.0.0:3000 --enable-tracy "$OUT_DIR/.spacetime" >"$OUT_DIR/spacetime_bots.log" 2>&1 &
SPACETIME_PROCESS=$!
sleep 5

target/release/spacetime call bitcraft nonexistent_reducer || echo "Call failed, as expected, but bitcraft module should be loaded"

$TRACY_CAPTURE_BIN -a ::1 -o "$OUT_DIR/output.tracy" >"$OUT_DIR/tracy-capture.log" 2>&1 &
TRACY_PROCESS=$!

REPLICAS=30 docker-compose -f "$BOTS_DIR/docker-compose.yml" up -d
echo "Letting bots run around a while"
sleep 300

docker-compose -f "$BOTS_DIR/docker-compose.yml" down

kill -INT "$SPACETIME_PROCESS" || echo "missing spacetime process?"
kill -INT "$TRACY_PROCESS" || echo "missing tracy process?"
sleep 30

ls -al "$OUT_DIR"
cat "$OUT_DIR/spacetime_bots.log"

zip -r "$OUT_DIR/bottest.zip" "$OUT_DIR/*.log" "$OUT_DIR/output.tracy"
