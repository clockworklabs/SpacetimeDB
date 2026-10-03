#!/usr/bin/env python3
"""Proposal 0040 spike: compile-time comparison at scale.

Writes, under OUT_DIR:
  module/         a module with N tables (primary key, unique column) and N reducers (two parameters)
  old-client/     a client crate; fill its module_bindings with today's `spacetime generate` output
  shaped-client/  the same client code against module-shaped bindings (one file, plus `client_module!`)

Usage, from the repository root:
  python3 sdks/rust/tests/module-shaped/scale/generate.py OUT_DIR [N=150]
  cargo build -p spacetimedb-cli -p spacetimedb-standalone
  SPACETIMEDB_SCHEMA_EXTRACTOR=$PWD/target/debug/spacetimedb-standalone ./target/debug/spacetimedb-cli \
      generate --lang rust --out-dir OUT_DIR/old-client/src/module_bindings --module-path OUT_DIR/module --no-config -y
  cd OUT_DIR && export CARGO_TARGET_DIR=<repo>/target CARGO_INCREMENTAL=0
  cargo build && touch old-client/src/main.rs && time cargo check -p scale-old-client
  touch shaped-client/src/main.rs && time cargo check -p scale-shaped-client

The module's accessors are `thing{i}` and `set_thing{i}`. Today's codegen converts them to `thing_{i}` and
`set_thing_{i}` (convert_case splits at digits), so the module-shaped bindings use those names, as codegen
would have to emit them to keep today's client API.
"""
import os
import sys

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "../../../../.."))
out = os.path.abspath(sys.argv[1])
n = int(sys.argv[2]) if len(sys.argv) > 2 else 150


def write(path, text):
    path = os.path.join(out, path)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)


def table(acc, i):
    return f"""#[spacetimedb::table(accessor = {acc}, public)]
pub struct Thing{i} {{
    #[primary_key]
    pub id: u64,
    #[unique]
    pub key: String,
    pub owner: Identity,
    pub value: u32,
}}
"""


module = "use spacetimedb::{Identity, ReducerContext, Table};\n\n"
shaped = "use spacetimedb_sdk as spacetimedb;\nuse spacetimedb::Identity;\n\n"
for i in range(n):
    module += table(f"thing{i}", i) + f"""
#[spacetimedb::reducer]
pub fn set_thing{i}(ctx: &ReducerContext, id: u64, value: u32) {{
    ctx.db.thing{i}().insert(Thing{i} {{ id, key: id.to_string(), owner: ctx.sender(), value }});
}}

"""
    shaped += table(f"thing_{i}", i) + f"""
#[spacetimedb::reducer]
pub fn set_thing_{i}(ctx: &spacetimedb::ReducerContext, id: u64, value: u32);

"""
shaped += "spacetimedb::client_module! {\n    tables: [" + ", ".join(f"thing_{i}" for i in range(n)) + "],\n"
shaped += "    reducers: [" + ", ".join(f"set_thing_{i}(id, value)" for i in range(n)) + "],\n}\n"

write("module/Cargo.toml", f"""[package]
name = "scale-module"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
spacetimedb = {{ path = "{REPO}/crates/bindings" }}
log = "0.4"

[workspace]
""")
write("module/src/lib.rs", module)

main = f"""mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{{DbContext, TableWithPrimaryKey}};

fn main() {{
    let conn = DbConnection::builder().with_uri("http://127.0.0.1:3000").with_database_name("scale").build();
    if let Ok(conn) = conn {{
        conn.db.thing_0().on_update(|_ctx, _old, new| println!("{{}}", new.value));
        let _ = conn.db.thing_{n - 1}().key().find(&"x".to_string());
        conn.reducers.set_thing_{min(7, n - 1)}(1, 2).unwrap();
        conn.subscription_builder().add_query(|q| q.from.thing_{min(3, n - 1)}()).subscribe();
        let r = Reducer::SetThing{min(42, n - 1)} {{ id: 1, value: 2 }};
        assert!(matches!(r, Reducer::SetThing{min(42, n - 1)} {{ .. }}));
    }}
}}
"""
for name in ["old-client", "shaped-client"]:
    write(f"{name}/Cargo.toml", f"""[package]
name = "scale-{name}"
version = "0.1.0"
edition = "2021"

[dependencies]
spacetimedb-sdk = {{ path = "{REPO}/sdks/rust" }}
""")
    write(f"{name}/src/main.rs", main)
write("shaped-client/src/module_bindings/mod.rs", shaped)
write("Cargo.toml", '[workspace]\nmembers = ["old-client", "shaped-client"]\nresolver = "2"\n')
print(f"wrote {out}")
