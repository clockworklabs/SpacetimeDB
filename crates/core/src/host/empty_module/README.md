# System empty module, version 1

The host uses this real Wasm module for a database published with a container and no user module. It declares V10 with an empty typespace and `hosted_auth_v1`; there are no user tables, reducers, procedures, views, schedules, or HTTP handlers.

`generate.py` is the complete source. It uses only the Python standard library and explicitly encodes the Wasm sections and BSATN description. It produces a 250-byte Wasm binary with one fixed memory page, a schema describer, and the required reducer ABI entry point. The reducer entry point always traps because no reducer is declared. The `spacetime_10.6::get_call_auth_flags` import requires the host ABI corresponding to the advertised capability; the empty module has no invocation contexts to construct.

From the public repository, reproduce and verify the checked-in files with:

```sh
python3 crates/core/src/host/empty_module/generate.py --check
```

Omit `--check` to regenerate the three files. This does not require a Rust module compiler, WASI toolchain, WAT compiler, or container image builder.

The SHA-256 checksum of `v1.wasm` is recorded in `v1.sha256`. SpacetimeDB's separate Keccak-256 program identity is:

```text
08365097d1ef202653615f02cc46be16088044c6b9cd9660e3a0f336e47918f0
```

Version 1 is immutable. A change to its schema or Wasm requires a new version and new files. Keep version 1 available for existing deployments and replay. The Rust helper checks the known program hash as well as the exact bytes, kind, and version, so a different publisher-supplied module with an empty-looking schema cannot qualify as the system empty module. `Program::empty` is also rejected.

Core tests compare the BSATN fixture against the current V10 Rust wire types, load the Wasm through the actual host, and initialize a real database through `HostController`. They verify that initialization stores the bundled program and the database's metadata without invoking a user reducer.
