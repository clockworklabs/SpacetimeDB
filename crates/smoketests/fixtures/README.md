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
