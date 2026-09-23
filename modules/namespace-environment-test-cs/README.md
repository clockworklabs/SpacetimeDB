# Namespace Environment Security Fixture

Run with `cargo test -p spacetimedb-testing --test environment namespace_csharp_environment_security`.
The test is also included by the `namespace_csharp` selector. The fixture requires .NET 10.

The `environment` suite shares `check_submodule_scope` between this test and
`typescript_environment_publish_and_checked_reads`. The case lists record each
language's entrypoint names and coverage: TypeScript additionally tests raw-SQL
bypass attempts; C# tests both sender and anonymous view contexts. C#-specific
declaration composition, root controls, and value-update checks stay in this test.

The root declares an optional environment key. AuthLib is registered in `MyAuth`;
PublicLib is discovered automatically and contributes an environment declaration
and procedure to `public`.

The test publishes a real value, replaces it, sets it to empty, and removes it. It verifies:

- Root reads and ordinary cross-assembly helpers retain root authority.
- Host-dispatched namespaced reducers, procedures (including transactions), and
  sender/anonymous views cannot read the root environment.
- A rejected call does not prevent subsequent root reads, or expose the value in errors.
- Root callers still cannot read undeclared keys.
- A dependency registered in `public` can declare and read environment keys.
- HTTP routes remain root entries, including dependency-defined routes; handlers
  and handler transactions retain root authority.

Root views intentionally expose the test value as a positive control. Child views
intentionally fail, so this fixture is separate from the general namespace client
regression that subscribes to all public tables and views. As in the TypeScript
environment tests, failed views may return no rows instead of an error (issue #5912).
