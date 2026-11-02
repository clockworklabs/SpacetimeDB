# Windows canonical path experiment — DO NOT MERGE

This draft is diagnostic only. It adds no fixes and is not intended to merge.
Failures are expected possibilities, not assertions that a tool must fail.

The standalone Rust tests invoke real tools with an ordinary path first, then
with `std::fs::canonicalize()` output. On Windows they assert that the latter
actually contains the extended-length prefix. Temporary paths contain spaces.
The formatter case checks the resulting file, not just its exit status.

The dedicated workflow runs on Windows even for a draft PR, with Linux as a
control. Rust's test harness runs every case and reports individual results;
neither test failures nor a failing operating-system job cancel the other cases.
No repository crates, server, WASI workloads, or application fixtures are needed.
NuGet access is needed by the .NET restore/pack controls.

| Probe | Repository boundary it models |
| --- | --- |
| Wildcard props import | MSBuild's import handling underlying #6449; compares ordinary and canonical project filenames |
| `dotnet pack` | `compile_csharp_client` in `crates/codegen/tests/codegen.rs` |
| Generated C# `ProjectReference` | The temporary client project in that same helper |
| `dotnet restore` of a netstandard2.0 project | Extra reference restore in `crates/cli/src/tasks/csharp.rs` for .NET 8 builds |
| `dotnet format whitespace --folder --include` | `dotnet_format` in the same CLI file |
| TypeScript `extends`, `paths`, and pnpm working directory, tested separately | `compile_typescript_receiver_bindings` in `crates/codegen/tests/codegen.rs` |
| Cargo path dependency | `crates/codegen/tests/outbox_builtin_shadowing.rs` |

These are reduced reproductions of tool boundaries, not end-to-end tests of those
repository functions. In particular, they do not prove that an entire CLI build,
or codegen test succeeds. They do not exercise UNC shares or paths exceeding
MAX_PATH. Those require
separate fixtures; the prefix appears on short Windows paths too.

The MSBuild mechanism has a related user report:
<https://github.com/dotnet/msbuild/issues/5407>.

To run locally, install Rust, .NET SDKs 8 and 10, pnpm 10.16.0, and TypeScript
5.6.3 on PATH, then:

```sh
rustc --edition=2021 --test tools/windows-path-probes/probes.rs -o path-probes.exe
./path-probes.exe --nocapture --test-threads=1
```
