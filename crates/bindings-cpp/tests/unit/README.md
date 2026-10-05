# C++ Unit Tests

Standalone unit-test harness for pure bindings/library behavior.

This suite is the right home for:
- conversion helpers
- small pure-library regressions
- behavior that does not need wasm module compilation
- behavior that does not need a live SpacetimeDB server

Current coverage includes the HTTP request/response split-body conversion checks that
mirror the Rust tests added next to `crates/bindings/src/http.rs`.

This harness is intentionally separate from the top-level bindings CMake so that
small header-only/library tests do not need to build the full module ABI/export layer.

It is built with Emscripten and run under Node, which matches the existing wasm-oriented
C++ test toolchain more closely than adding a separate native-MSVC path.

The generated Node launcher uses a `.cjs` suffix so it is treated as CommonJS even though
the repo root sets `"type": "module"`.

## Run

Prerequisites:

- `emcmake` on `PATH`
- `node` on `PATH`

From PowerShell:

```powershell
.\crates\bindings-cpp\tests\unit\run-unit-tests.ps1
```

Verbose:

```powershell
.\crates\bindings-cpp\tests\unit\run-unit-tests.ps1 -Detailed
```

From Git Bash:

```bash
./crates/bindings-cpp/tests/unit/run-unit-tests.sh
```

Verbose:

```bash
./crates/bindings-cpp/tests/unit/run-unit-tests.sh --verbose
```
