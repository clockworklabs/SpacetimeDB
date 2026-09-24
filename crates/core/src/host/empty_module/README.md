# Container-only platform module

Version 2 is generated in `spacetimedb-lib::deployment::system_empty` from the validated declarations in container configuration. It exports the required V10 ABI entry points and declares ENV section15 and capability section16. It has no user tables, reducers, procedures, views, or HTTP handlers.

Declarations are normalized before generation. Their exact bytes determine the Wasm program hash; changing declarations therefore selects a new module. The generated memory has equal minimum and maximum page counts, sized to contain the complete bounded metadata, including schemas larger than one Wasm page.

The shared verifier reads only bounded declaration data, regenerates the platform program, and compares every byte and the claimed program identity. It never executes the supplied program to decide whether it is platform code. The unshipped version1 prototype is not accepted.

Run the canonical generator and protocol tests with `cargo test -p spacetimedb-lib --lib deployment::`. The actual host extraction and initialization tests are in `crates/core/src/host/empty_module/tests.rs`.
