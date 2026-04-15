# A brief overview of SpacetimeDB testing
## From the perspective of a client SDK or module library developer
## By pgoldman 2025-06-25, updated 2026-04-14

SpacetimeDB has good test coverage, but it is rather haphazardly spread across several suites.
Some of the reasons for this are historical, and some have to do with our using multiple repositories.
This document is an attempt to describe the test suites which would be useful to someone
working on a new client SDK or module bindings library in a new language,
or attempting to move an existing client SDK or module bindings library from an external repository in-tree.

## The SDK tests

`crates/testing/src/sdk.rs` defines a test harness which was originally designed for testing client SDKs.
The basic flow of a test using this harness is:

- Build and freshly publish a module to construct a short-lived initially-empty database.
- Use that module to run client codegen via `spacetime generate` into a client project.
- Compile that client project with the newly-generated bindings.
- Run the client project as a subprocess, passing the database name or `Identity` in an environment variable.
- The client process connects to the database,
  runs whatever tests it likes,
  writes to stdout and/or stderr as it goes,
  then uses its exit code to report whether the test was successful or not.
- If the subprocess's exit is non-zero, the test is treated as a failure,
  and the subprocess's stdout and stderr are reported.

This framework has since been used more generally for integration testing.
In particular, we maintain equivalent Rust, C#, TypeScript, and C++ modules in the `modules/sdk-test*` family,
and run the Rust SDK client project at `sdks/rust/tests/test-client` against them through `sdks/rust/tests/test.rs`.
We similarly maintain `modules/sdk-test-connect-disconnect*` modules
which run against `sdks/rust/tests/connect_disconnect_client`.
There are also related SDK-harness-driven suites for event tables, procedures, and views,
using modules such as `modules/sdk-test-event-table`, `modules/sdk-test-procedure*`, and `modules/sdk-test-view*`.
The Unreal SDK also uses the same underlying harness through `sdks/unreal/tests/sdk_unreal_harness.rs`.

The harness is designed to support running multiple tests in parallel with the same client project,
running client codegen exactly once per test suite run.
This unfortunately still conflicts with our use of the suite to test that modules in different languages behave the same,
as each test suite invocation will only run `spacetime generate` against one module language at a time,
never all of them in the same run.

### Testing a new module library

If you are developing a new module bindings library, and wish to add it to the SDK test suite
so that the existing client test projects will run against it:

1. Create `modules/sdk-test-XX` and `modules/sdk-test-connect-disconnect-XX`, where `XX` is some mnemonic for your language.
   Populate these with module code which defines all of the same tables and reducers
   as `modules/sdk-test` and `modules/sdk-test-connect-disconnect` respectively.
   Take care to use the same names, including casing, for tables, columns, indexes, reducers and other database objects.
2. Modify `sdks/rust/tests/test.rs` to add an additional call to `declare_tests_with_suffix!` at the bottom,
   like `declare_tests_with_suffix!(xxlang, "-XX")`, if that driver is the right place for the new language.
   Some capabilities now live in separate suites in that file, such as procedures and views,
   so you may need to wire those up separately as well.
3. Run the tests with `cargo test -p spacetimedb-sdk --test test`.

### Testing a new client SDK

If you are developing a new client SDK, and wish to use the SDK test harness and existing modules
so that it will run against `modules/sdk-test` and `modules/sdk-test-connect-disconnect`:

1. Find somewhere sensible to define test projects `test-client` and `connect_disconnect_client` for your client SDK language.
   If your client SDK is in-tree, put these within its directory, following the existing layout under `sdks/rust/tests/` or `sdks/unreal/tests/`.
2. Use `spacetime generate` manually, or via the harness, to generate those projects' `module_bindings`.
3. Populate those projects with client code
   matching `sdks/rust/tests/test-client` and `sdks/rust/tests/connect_disconnect_client` respectively.
   - Connect to SpacetimeDB running at `http://localhost:3000`.
   - Connect to the database whose name is in the environment variable `SPACETIME_SDK_TEST_DB_NAME`.
   - For `test-client`, take a test name as a command-line argument in `argv[1]`, and dispatch to the appropriate test to run.
   - For `connect_disconnect_client`, there is only one test.
   - The Rust code jumps through some hoops to do assertions about asynchronous events with timeouts,
     using an abstraction called the `TestCounter` defined in `sdks/rust/tests/test-counter`.
     This is effectively a semaphore with a timeout.
     You may or may not need to replicate this behavior.
4. Create integration tests in the SDK crate which construct `spacetimedb_testing::sdk::Test` objects,
   following `sdks/rust/tests/test.rs` or `sdks/unreal/tests/test.rs` as a template.
5. Define `#[test]` tests for each test case you have implemented,
   which construct `spacetimedb_testing::sdk::Test` objects containing the various subcommand strings to run your client project,
   then call `.run()` on them.

### Adding a new test case

If you want to add a new test case to the SDK test suite, to test some new or yet-untested functionality
of either the module libraries or client SDKs:

1. If necessary, add new tables and/or reducers to `modules/sdk-test` and friends which exercise the behavior you want to test.
2. Add a new function, `exec_foo`, to the appropriate client project,
   such as `sdks/rust/tests/test-client/src/lib.rs`,
   which connects to the database, subscribes to tables and invokes reducers as appropriate,
   and performs assertions about the events it observes.
3. Add a branch to that client's dispatch logic,
   such as the `match` in `sdks/rust/tests/test-client/src/main.rs`,
   which matches the test name `foo` and dispatches to call your `exec_foo` function.
4. Add a `#[test]` test function to the relevant test driver,
   such as `sdks/rust/tests/test.rs`,
   which does `make_test("foo").run()`, where `"foo"` is the test name you chose in step 3.
5. Repeat steps 2 through 4 for any other client projects which ought to cover the same behavior.
6. Run the new test with the relevant `cargo test` command for that SDK.

## Schema parity tests

`crates/schema/tests/ensure_same_schema.rs` is a separate but important companion to the SDK tests.
It compares the extracted schemas of equivalent modules across languages,
and is often the first place where casing, indexes, primary keys, or other schema details drift apart.
As of writing, it covers the `benchmarks`, `module-test`, `sdk-test`, and `sdk-test-connect-disconnect` families.

If you add or update a cross-language module family,
it is worth considering whether it should also be covered here.

## The smoketests

`crates/smoketests/` defines an integration and regression test suite using a Rust harness.
These are useful primarily for testing the SpacetimeDB CLI, but can also be used to exercise publish flows,
documentation, and other end-to-end behavior.

The smoketest harness is still primarily oriented around Rust modules,
and it does not use the same client-project machinery as the SDK harness.
It could be extended to do more in that direction, but that may not be worth the effort.
As of writing, the smoketest suite includes dedicated coverage such as:

- `crates/smoketests/tests/smoketests/csharp_module.rs`, which smoke-tests C# module compilation.
- `crates/smoketests/tests/smoketests/quickstart.rs`, which replays the quickstart guide for Rust and C#.
- `crates/smoketests/DEVELOP.md`, which documents how to run and write these tests.

One practical note is that the smoketests use prebuilt `spacetimedb-cli` and `spacetimedb-standalone` binaries,
so if you modify those crates or their dependencies, you should rebuild before running the suite.

## Standalone integration test

The `spacetimedb-testing` crate has an integration test file, `crates/testing/tests/standalone_integration_test.rs`.
The tests in this file publish `modules/module-test`, `modules/module-test-cs`, `modules/module-test-ts`, and `modules/module-test-cpp`,
then invoke reducers or procedures in them and inspect their logs to verify that the behavior is expected.
These tests do not exercise the entire functionality of `module-test`,
but by virtue of publishing it do assert that it is syntactically valid and that it compiles.

To add a new module library to the Standalone integration test suite:

1. Create `modules/module-test-XX`, where `XX` is some mnemonic for your language.
2. Populate this with module code which defines all of the same tables and reducers
   as the existing `module-test` family.
   If you notice any discrepancies between the existing languages, those parts may be compiled but not run,
   and so you are free to ignore them.
3. Modify `crates/testing/tests/standalone_integration_test.rs` to define new `#[test] #[serial]` test functions
   which use your new `module-test-XX` module to do the same operations as the existing tests.
4. Run the tests with `cargo test -p spacetimedb-testing --test standalone_integration_test`.