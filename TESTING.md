# A brief overview of SpacetimeDB testing
## From the perspective of a client SDK or module library developer
## By pgoldman 2025-06-25

SpacetimeDB has good test coverage, but it's rather haphazardly spread across several suites.
Some of the reasons for this are historical, and some have to do with our using multiple repositories.
This document is an attempt to describe the test suites which would be useful to someone
developing a new client SDK or module bindings library in a new language,
or attempting to move an existing client SDK or module bindings library from an external repository in-tree.

## The SDK tests

`crates/testing/src/sdk.rs` defines a test harness which was originally designed for testing client SDKs.
The basic flow of a test using this harness is:

- Build and freshly publish a module to construct a short-lived initially-empty database.
- Use that module to run client codegen via `spacetime generate` into  a client project.
- Compile that client project with the newly-generated bindings.
- Run the client project as a subprocess, passing the database name or `Identity` in an environment variable.
- The client process connects to the database,
  runs whatever tests it likes,
  writes to stdout and/or stderr as it goes,
  then uses its exit code to report whether the test was successful or not.
- If the subprocess' exit is non-zero, the test is treated as a failure,
  and the subprocess' stdout and stderr are reported.

This framework has since been used more generally for integration testing.
In particular, we maintain identical Rust and C# modules in `modules/sdk-test` and `modules/sdk-test-cs`,
and run the same client project, `crates/sdk/tests/test-client` against both.
We similarly maintain `modules/sdk-test-connect-disconnect` and `modules/sdk-test-connect-disconnect-cs`
which run against `crates/sdk/tests/connect_disconnect_client`.

The harness is designed to support running multiple tests in parallel with the same client project,
running client codegen exactly once per test suite run.
This unfortunately conflicts with our use of the suite to test that the Rust and C# modules behave the same,
as each test suite invocation will only run `spacetime generate` against either `modules/sdk-test` or `modules/sdk-test-cs`,
never both in the same run.

### Testing a new module library

If you're developing a new module bindings library, and wish to add it to the SDK test suite
so that `crates/sdk/tests/test-client` and `crates/sdk/tests/connect_disconnect_client` will run against it:

1. Create `modules/sdk-test-XX` and `modules/sdk-test-connect-disconnect-XX`, where `XX` is some mnemonic for your lanugage.
   Populate these with module code which define all of the same tables and reducers
   as `modules/sdk-test` and `modules/sdk-test-connect-disconnect` respectively.
   Take care to use the same names, including casing, for tables, columns, indexes, reducers and other database objects.
2. Modify `crates/sdk/tests/test.rs` to add an additional call to `declare_tests_with_suffix!` at the bottom,
   like `declare_tests_with_suffix!(xxlang, "-XX")`.
3. Run the tests with `cargo test -p spacetimedb-sdk -- xxlang`.

### Testing a new client SDK

If you're developing a new client SDK, and which to use the SDK test harness and existing modules
so that it will run against `modules/sdk-test` and `modules/sdk-test-connect-disconnect`:

1. Find somewhere sensible to define test projects `test-client` and `connect_disconnect_client` for your client SDK language.
   If your client SDK is in-tree, put these within its directory; otherwise, put them in `crates/testing/tests/sdk-test-clients/xxlang/`.
2. Use `spacetime generate` manually to generate those projects' `module_bindings`.
3. Populate those projects with client code
   matching `crates/sdk/tests/test-client` and `crates/sdk/tests/connect_disconnect_client` respectively.
   - Connect to SpacetimeDB running at `http://localhost:3000`.
   - Connect to the database whose name is in the environment variable `SPACETIME_SDK_TEST_DB_NAME`.
   - For `test-client`, take a test name as a command-line argument in `argv[1]`, and dispatch to the appropriate test to run.
   - For `connect_disconnect_client`, there's only one test.
   - The Rust code jumps through some hoops to do assertions about asynchronous events with timeouts,
     using an abstraction called the `TestCounter` defined in `crates/sdk/tests/test-counter/lib.rs`.
     This is effectively a semaphore with a timeout.
     You may or may not need to replicate this behavior.
4. Create a Cargo integration test file within the `testing` crate at `crates/testing/sdk-test-xxlang.rs`.
5. Using `crates/sdk/tests/test.rs` as a template, define `#[test]` tests for each test case you've implemented,
   which construct `spacetimedb_testing::sdk::Test` objects containing the various subcommand strings to run your client project,
   then call `.run()` on them.

### Adding a new test case

If you want to add a new test case to the SDK test suite, to test some new or yet-untested functionality
of either the module libraries or client SDKs:

1. If necessary, add new tables and/or reducers to `modules/sdk-test` and friends which exercise the behavior you want to test.
2. Add a new function, `exec_foo`, to `crates/sdk/tests/test-client/src/lib.rs` which connects to the database,
   subscribes to tables and invokes reducers as appropriate, and performs assertions about the events it observes.
3. Add a branch to the `match &*test` in that file's `fn main`
   which matches the test name `foo` and dispatches to call your `exec_foo` function.
4. Add a `#[test]` test function to the definition of the `declare_tests_with_suffix` macro
   in `crates/sdk/tests/test.rs` which does `make_test("foo").run()`, where `"foo"` is the test name you chose in step 3.
5. Repeat steps 2 through 4 for any other client projects which have been added since writing.
6. Run the new test with `cargo test -p spacetimedb-sdk -- foo`.

## The smoketests

`smoketests/` defines an integration/regression test suite using a harness written in Python based around the Python `unittest` library. These are useful primarily for testing the SpacetimeDB CLI, but can also be used to run arbitrary commands.

The test harness currently assumes that tested modules are written in Rust, and does not have any machinery for running client projects. It could be extended to fix either of these shortcomings, but that may not be worth the effort. As of writing, the only smoketest we have which interacts with C# modules is `smoketests/tests/csharp_module.py`, which uses `spacetime build` to compile a C# module project but does not publish it.

## Standalone integration test

The `spacetimedb-testing` crate has an integration test file, `crates/testing/tests/standalone_integration_test.rs`.
The tests in this file publish `modules/module-test` and `modules/module-test-cs`,
then invoke reducers in them and inspect their logs to verify that the behavior is expected.
These tests do not exercise the entire functionality of `module-test`,
but by virtue of publishing it do assert that it is syntactically valid and that it compiles.

To add a new module library to the Standalone integration test suite:

1. Create `modules/module-test-XX`, where `XX` is some mnemonic for your language.
2. Populate this with module code which defines all of the same tables and reducers
   as `modules/module-test` and `modules/module-test-cs`.
   If you notice any discrepancies between the two existing languages, those parts are compiled but not run,
   and so you are free to ignore them.
3. Modify `crates/testing/tests/standalone_integration_test.rs` to define new  `#[test] #[serial]` test functions
   which use your new `module-test-XX` module to do the same operations as the existing tests.
4. Run the tests with `cargo test -p spacetimedb-testing`.
