# SpacetimeDB C++ Compile Tests

Focused compile-surface regression tests for the C++ bindings.

This harness is intended for:
- authoring-time success cases
- compile-fail regression cases
- API surface checks that should fail before publish/runtime

## HTTP Handler Coverage

The `http-handlers` suite mirrors the Rust coverage in
`crates/bindings/tests/ui/http_handlers.rs` as closely as the C++ macro surface allows.

Covered cases:
- valid handler/router authoring
- no handler args
- immutable handler context
- wrong handler context type
- missing request arg
- wrong request arg type
- missing return
- wrong return type
- forbidden `HandlerContext::sender()`
- forbidden `HandlerContext::connection_id`
- forbidden `HandlerContext::db`
- router authored with args
- router wrong return type
- router misuse in a non-function position

## Run

From Git Bash or Linux-style shells:

```bash
./crates/bindings-cpp/tests/compile/run-compile-tests.sh --suite http-handlers
```

From PowerShell at the repo root:

```powershell
.\crates\bindings-cpp\tests\compile\run-compile-tests.ps1 -Suite http-handlers
```

Or from the compile test directory:

```powershell
.\run-compile-tests.ps1 -Suite http-handlers
```

## Output

Build artifacts and logs are written under:

```text
crates/bindings-cpp/tests/compile/build/
```

Each case gets:
- `build/<case>/configure.log`
- `build/<case>/build.log`

The shared bindings library build is under:
- `build/library/`
