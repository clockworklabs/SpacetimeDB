# Smoketest Fixtures

`upgrade_old_module_v1.wasm` is an old-format module fixture used by
`crates/smoketests/tests/publish_upgrade_prompt.rs` to test the `1.0 -> 2.0`
upgrade confirmation flow.

It was produced from a pre-`RawModuleDefV10` bindings snapshot and exports
`__describe_module__` (not `__describe_module_v10__`).

## Regenerate

```bash
# from repo root
TMP="$(mktemp -d)"
git archive --format=tar d3f59480e -o "$TMP/old-repo.tar"
mkdir -p "$TMP/old-repo"
tar -xf "$TMP/old-repo.tar" -C "$TMP/old-repo"

mkdir -p "$TMP/old-module/src"
cat > "$TMP/old-module/Cargo.toml" <<EOF
[package]
name = "upgrade_old_module"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
spacetimedb = { path = "$TMP/old-repo/crates/bindings", features = ["unstable"] }
EOF

cat > "$TMP/old-module/src/lib.rs" <<'EOF'
use spacetimedb::{reducer, ReducerContext};

#[reducer]
pub fn noop(_ctx: &ReducerContext) {}
EOF

CARGO_NET_OFFLINE=true CARGO_TARGET_DIR="$TMP/target-old" \
  cargo build --release --target wasm32-unknown-unknown \
  --manifest-path "$TMP/old-module/Cargo.toml"

cp "$TMP/target-old/wasm32-unknown-unknown/release/upgrade_old_module.wasm" \
  crates/smoketests/fixtures/upgrade_old_module_v1.wasm
```

## `stale-view-backing-table-v2.6.0`

This is a standalone data-dir fixture created by `spacetimedb-standalone` `v2.6.0`.
It contains database identity
`c200f6ec405075e508c2ed6474019332d6a2a46c69614306cc4bd980e0b8b767`, published
from `crates/smoketests/modules/views-sql`.

The fixture exists to cover startup repair for view backing tables created before
the `arg_hash` internal column. The persisted sender-scoped view backing tables
begin with `sender`, and anonymous view backing tables do not have an internal
argument column.

The checked-in fixture intentionally keeps only the control database entry and
commitlog segment required to replay that database state. The current server can
recreate config, metadata, commitlog offset indexes (`*.stdb.ofs`), snapshots,
lock files, and program byte storage from defaults and the commitlog during
startup.

To regenerate it:

```bash
TMP="$(mktemp -d)"
git archive --format=tar v2.6.0 | tar -xf - -C "$TMP"

cargo build --manifest-path "$TMP/Cargo.toml" \
  -p spacetimedb-standalone --release \
  --features spacetimedb-standalone/allow_loopback_http_for_tests

"$TMP/target/release/spacetimedb-standalone" start \
  --data-dir "$TMP/data" \
  --jwt-key-dir "$TMP/keys" \
  --listen-addr 127.0.0.1:0 \
  --non-interactive

# In another shell, publish to the printed server URL.
CARGO_HOME="$TMP/cargo-home" CARGO_TARGET_DIR="$TMP/module-target" \
  target/release/spacetimedb-cli --config-path "$TMP/client-config.toml" publish \
  --server "http://127.0.0.1:<PORT>" \
  --module-path crates/smoketests/modules/views-sql \
  --yes stale-view-backing-table-v26

# Stop the old server, update the identity constant if it changed, then copy the
# minimal fixture.
FIXTURE=crates/smoketests/fixtures/stale-view-backing-table-v2.6.0
rsync -am \
  --include='*/' \
  --include='control-db/db' \
  --include='replicas/1/clog/*.stdb.log' \
  --exclude='*' \
  "$TMP/data/" "$FIXTURE/"
```
