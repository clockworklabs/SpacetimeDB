#!/usr/bin/env bash
# Build the SpacetimeDB CLI for Linux, from THIS repository, so a containerised
# build can publish modules with the CLI actually under test.
#
# The benchmark's whole claim about SpacetimeDB rests on measuring the software
# in this checkout rather than a published release. `target/release/
# spacetimedb-cli.exe` is a Windows PE binary and a Linux container cannot run
# it, so without this the SpacetimeDB backend falls back to running on the host
# and loses the isolation every other backend gets.
#
# Outputs: tools/stack-bench/container/bin/spacetimedb-cli and
#          tools/stack-bench/container/bin/spacetimedb-standalone (Linux ELFs)
#
# Usage: bash tools/stack-bench/container/build-linux-cli.sh
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../../.." && pwd)"
OUT="$HERE/bin"

# Git Bash rewrites container-side paths into Windows ones and every mount lands
# somewhere wrong.
export MSYS_NO_PATHCONV=1

# This digest is the linux/amd64 manifest for rust:1.93-slim-bookworm. Update it
# with rust-toolchain.toml and the expected image in binary-provenance.mjs.
IMAGE="rust:1.93-slim-bookworm@sha256:8f8609d448e821fbc0e44241bc5ca4ce49663cc6306ff1a17f655a0e2a7cd084"

# Cargo's target directory is a named volume, not a path in the repo. A Rust
# build against a Windows bind mount is many times slower, and it would also sit
# next to the Windows artifacts in target/ where the wrong one is easy to pick up
# so container runs cannot silently execute a stale or host-platform binary.
VOLUME="${STACK_BENCH_CARGO_VOLUME:-stack-bench-cargo-target}"

SOURCE_RECORD="$(mktemp)"
trap 'rm -f "$SOURCE_RECORD"' EXIT
node "$HERE/binary-provenance.mjs" source --repo "$REPO" >"$SOURCE_RECORD"

mkdir -p "$OUT"
docker volume create "$VOLUME" >/dev/null

echo "building spacetimedb-cli for linux (image $IMAGE, target volume $VOLUME)"
echo "  first build compiles the whole workspace and takes a while; later ones reuse the volume"

docker run --rm --platform linux/amd64 \
  -v "$REPO:/src" \
  -v "$VOLUME:/target" \
  -v "$OUT:/out" \
  -w /src \
  -e CARGO_TARGET_DIR=/target \
  -e CARGO_TERM_COLOR=never \
  "$IMAGE" \
  bash -c '
    set -euo pipefail
    # pkg-config/libssl: the CLI links openssl. clang/cmake: some transitive
    # build scripts need them. Installed here rather than baked into an image so
    # this script stays a single file with nothing to keep in sync.
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends \
      pkg-config libssl-dev build-essential clang cmake perl git curl python3 >/dev/null
    rustup show >/dev/null            # honours rust-toolchain.toml
    cargo build --release --locked \
      -p spacetimedb-cli --bin spacetimedb-cli \
      -p spacetimedb-standalone --bin spacetimedb-standalone
    cp /target/release/spacetimedb-cli /out/spacetimedb-cli
    cp /target/release/spacetimedb-standalone /out/spacetimedb-standalone
    chmod +x /out/spacetimedb-cli /out/spacetimedb-standalone
  '

echo ""
file "$OUT/spacetimedb-cli" 2>/dev/null || true
file "$OUT/spacetimedb-standalone" 2>/dev/null || true
docker run --rm -v "$OUT:/deps:ro" "${STACK_BENCH_IMAGE:-stack-bench-build:2.1.226}" \
  sh -c 'test -x /deps/spacetimedb-standalone && /deps/spacetimedb-cli --version'

node "$HERE/binary-provenance.mjs" record --repo "$REPO" --source-file "$SOURCE_RECORD"
