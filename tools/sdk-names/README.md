# SDK name manifest

`manifest.toml` is the name manifest of proposal 0041, the SDK naming scheme. It has one entry per role that an SDK item plays, such as `client.connection` or `module.context.reducer`, and records that role's name in every module library and client SDK: Rust, TypeScript, and C# modules and clients, the Unreal client, and the C++ module library. It also records the deprecated names that must still resolve, and each place where a language deviates from the scheme, with the reason.

The manifest covers the roles in the proposal's catalog and the user-facing types and traits of the connection, contexts and events, client views, table and index handles, subscriptions, query builder, and module contexts. Methods, values, and traits that only exist to add a method, such as Rust's per-table accessor traits, are out of scope.

`cargo ci sdk-names` generates, from the manifest, one conformance test per language that names every current and deprecated name, so that the build fails if a name disappears. It also checks that every name follows the scheme or records a deviation (see [Name check](#name-check)). If a generated test was out of date, it rewrites the test and then fails, as `cargo ci cli-docs` does, so that a plain `cargo ci` cannot pass with stale tests; commit the rewritten tests. `cargo ci sdk-names --check`, which CI runs in the lint job, writes nothing and fails if a generated test is out of date.

## The naming scheme

The scheme rests on one rule: name the role, not the representation. Two items that play the same role get the same name, even when one language represents the role as a trait, another as a type, and a third as a value. A role's name is built from the vocabulary below, and each language renders it by its rendering rules.

### Vocabulary

Each term has one meaning, and a test that decides whether a new item takes it.

| Term | Meaning | Test |
|---|---|---|
| `…Decl` | An item as the author declares it, or as an SDK builds it from what the author wrote, such as `TableDecl`. It has not been validated, even if an SDK has checked parts of it. A part of a declaration is named for its role instead, such as `TableBody`, the declaration of a table without its accessor. | Did the author write it, or did an SDK build it from what the author wrote? |
| `Raw…Def` | The input to the host's validator, which a module library produces from its declarations, such as `RawModuleDefV10`. | Is it what the validator consumes? |
| `…Def` | The output of the host's validator, such as `ModuleDef`. An SDK's checks do not make a definition, so an SDK's own types are not named `…Def`. | Did the validator produce it? |
| `…Schema` | The catalog's representation of part of the schema, as the database stores it, such as the host's `TableSchema`. Only the host uses it. | Is it the catalog's representation of an object? |
| `Remote…` | The client's view of something that a module defines, such as `RemoteTables`. A module never sees one. | Is it on the client side, and does it stand for something the module defines? |
| `Untyped…` | The widest instantiation of a generic type, used as a constraint, such as `UntypedRemoteModuleDecl`. Only TypeScript uses it: Rust uses a trait with the base name, and C# and Unreal a `…Base` class. | Is it a generic type's upper bound? |
| `…Base` | The SDK's generic or untyped type that generated code specializes for one module, by subclassing, wrapping, or instantiating it, such as `DbConnectionBase`. | Does generated code build a per-module type from it? |
| `…Impl` | A private implementation of a public trait or interface, such as TypeScript's internal `ReducerContextImpl`. | Is it hidden from users and from generated code? |
| `…Args` | The arguments of one reducer or procedure call, as a single type, such as `{Reducer}Args`. | Is it one call's arguments? |
| `…Handle` | An object that acts on stored data at runtime, by inserting, deleting, finding, or iterating, such as `TableHandle`. | Does it perform operations on rows? |
| `…Index` | A handle to one index of a table, named by what it can do: `UniqueIndex` finds one row, `PointIndex` filters by a value, and `RangedIndex` filters by a range. | Is it a handle scoped to a single index? |
| `…Context` | The object passed to a function or callback that gives it access to the database, the caller, and the time of the call, such as `ReducerContext` and `ErrorContext`. Always `Context`, never `Ctx`, including in names built from it. | Is it the first argument of a function or callback? |
| `ReadOnly…` | The read-only variant of an item, such as `ReadOnlyDbView`. It is always a prefix. | Does it offer a subset of another item's operations, without writes? |
| `…Builder` | An object that is configured and then produces another object, such as `DbConnectionBuilder`. | Does it end in a `build`, `subscribe`, or similar call that produces the thing it configures? |
| `…Event` | A record of something that happened, delivered to a callback, such as `ReducerEvent`. A context may contain one. | Does it describe an occurrence rather than provide access? |
| `DbView` | The database as a module function sees it, the type of `ctx.db`. The client's view of the database is `RemoteTables`. | Is it the type of a module's `ctx.db`? |

Names also follow these rules:

* A generated name for one item puts the item's name first, `{Item}{RoleName}`, as in `PersonTableHandle` and `PersonCols`.
* A name inside a SpacetimeDB package does not repeat the package name, as `SpacetimeModule` inside `spacetimedb_sdk` did. A language without namespaces, such as Unreal, is the exception: its core types are named like `FSpacetimeDBIdentity`.
* Where a name includes the product name, it is spelled `SpacetimeDB`, or `spacetimedb` in lowercase identifiers, and never abbreviated to `Stdb` or `STDB`.
* A name does not end in a word for its kind of declaration, such as `Interface`.

### Rendering rules

| | Rust | TypeScript | C# | Unreal C++ | C++ module |
|---|---|---|---|---|---|
| Types | `UpperCamelCase` | `PascalCase` | `PascalCase` | `PascalCase` with Unreal's `U` (UObject), `F` (struct), or `E` (enum) prefix | `PascalCase` |
| Interfaces | trait with the base name | type with the base name | `I` prefix | base class with the `Base` suffix, as in `UDbConnectionBase` | none |
| Values and functions | `snake_case` | `camelCase`; generated module constants `SCREAMING_SNAKE_CASE` | `PascalCase` methods | `PascalCase` | `snake_case` |
| A family of types for one item | trait, implemented by a marker type | generic data type, with an `Untyped…` constraint | generic type parameters on `…Base` classes | generated subclasses of `…Base` classes | templates |
| Read-only prefix | `ReadOnly` | `Readonly` | `ReadOnly` | `ReadOnly` | `ReadOnly` |
| Generated prefix for one module | none | none | none | the module prefix, as in `U{Module}RemoteTables` | none |

A rename keeps the old name working where the language can express a deprecated alias: in Rust, a `#[deprecated]` type alias, or a hidden re-export for a trait; in TypeScript, a `@deprecated` type alias, plus `export const Old = New` for a class; in C++, `using Old [[deprecated]] = New`, or an alias template for a template. C# has no exported type aliases, and reflected Unreal types cannot be aliased, so their renames wait for a major version.

## Format

Each `[[role]]` has:

* `id`, a stable identifier. Where the proposal's catalog gives one identifier several names, as in `module.context.*` or `client.connection`, each name gets its own entry under that identifier, as in `module.context.reducer` and `client.connection.builder`.
* `name`, the role's name under the scheme. A role whose name the vocabulary does not determine has none, and is listed under [Open naming questions](#open-naming-questions).
* `description`, one line.
* One key per column: `rust_module`, `rust_client`, `ts_module`, `ts_client`, `csharp_module`, `csharp_client`, `unreal`, and `cpp_module`. Each is either `"none"`, when the language has no such item, or an inline table:
  * `sdk = "Name"` for a name that the SDK or module library defines, or `generated = "Name"` for a name that generated code defines. A generated name can be a pattern, such as `{Table}TableHandle`.
  * `path`, where the name lives, if not in the column's default location (see below).
  * `deprecated`, former names that must still resolve. A cell with only deprecated names records a role whose current name the language has removed.
  * `cfg`, a Rust `cfg` predicate that the name needs, such as `'feature = "unstable"'`.
  * `kind = "class"`, for a TypeScript class, whose current and deprecated names must also resolve as values.
  * `unchecked`, the reason no test can name the item.
* `deviations`, an inline table from column to a one-line reason, for each column whose name departs from the scheme.
* `note`, an optional line of context.

C# and C++ names of generic types and templates include their parameters, as in `DbConnectionBase<DbConnection, Tables, Reducer>` and `TableAccessor<T>`. C#'s `typeof` needs their number, and the C++ case compares a deprecated alias template with its replacement by instantiating both with a distinct type for each parameter, so that an alias that reorders or repeats parameters fails.

The `[instances.<column>]` tables fill in the placeholders of generated names, such as `{Table}` and `{Row}`, with an item from the project that each column's generated names are checked against.

## Name check

For every role with a `name`, each column's current name must follow that name or have a deviation, and a name that follows it must not have one. The check compares the two as lists of lowercase words, so casing conventions don't matter (`ReadOnlyDbView` matches `read_only_db_view`), and a placeholder such as `{Table}` counts as one word. Before comparing, it undoes each language's rendering rules:

* In every language, it takes the last segment of a nested name, so `RemoteTables.{Table}Handle` gives `{Table}Handle`, and drops generic or template parameters.
* In C#, it drops the `I` of an interface, so `IDbContext` gives `DbContext`.
* In Unreal, it drops the `U`, `F`, or `E` prefix, then the module prefix `{Module}`, then a `SpacetimeDB` prefix, so `F{Module}Event` gives `Event`.
* In TypeScript, it reads a `Readonly` prefix as `ReadOnly`.

An SDK type serves every item, so an SDK name is compared with the scheme name without its placeholders: TypeScript's `TableHandle` follows `{Table}TableHandle`. A generated name must match the placeholders too, so C#'s `{Table}Cols` does not follow `{Row}Cols`. Deprecated names, and roles without a name, are not checked.

## Where each language is checked

| Column | SDK names | Generated names | Default location | Run locally |
|---|---|---|---|---|
| `rust_module` | `crates/bindings/tests/sdk_names.rs` | the same file, which declares a table | `spacetimedb`; generated names in the test's own module | `cargo test -p spacetimedb --features unstable --test sdk_names` |
| `rust_client` | `sdks/rust/tests/sdk_names.rs` | `sdks/rust/tests/view-client/src/sdk_names.rs`, against its checked-in bindings | `spacetimedb_sdk`; generated names in `crate::module_bindings` | `cargo test -p spacetimedb-sdk --test sdk_names` and `cargo build -p view-client` |
| `ts_module` | `crates/bindings-typescript/src/lib/sdk_names_module.test-d.ts` | none | the `spacetimedb/server` entry point, or a source file such as `src/lib/table.ts` | `pnpm build` in `crates/bindings-typescript` |
| `ts_client` | `crates/bindings-typescript/src/lib/sdk_names_client.test-d.ts` | `crates/bindings-typescript/test-app/src/sdk_names.test-d.ts`, against its checked-in bindings | the `spacetimedb` entry point; generated names in `module_bindings` | `pnpm build` in `crates/bindings-typescript`, then `pnpm generate && pnpm build` in its `test-app` |
| `csharp_module` | `crates/bindings-csharp/Codegen.Tests/fixtures/server/SdkNames.g.cs`, compiled with the module generators by `Codegen.Tests` | the same file | namespace `SpacetimeDB` | `dotnet test crates/bindings-csharp/Codegen.Tests` |
| `csharp_client` | `sdks/csharp/examples~/regression-tests/client/SdkNames.g.cs`, compiled by the C# regression tests | the same file, against its checked-in bindings | namespace `SpacetimeDB`; generated names in `SpacetimeDB.Types` | `dotnet build "sdks/csharp/examples~/regression-tests/client/client.csproj"` |
| `cpp_module` | `crates/bindings-cpp/tests/compile/cases/indexes/ok_sdk_names.cpp`, in the `indexes` compile suite | the same file, which declares a table | namespace `SpacetimeDB`; generated names in the global namespace | `crates/bindings-cpp/tests/compile/run-compile-tests.sh --suite indexes`, with emsdk's `emcmake` on the `PATH` and bash 4 or later |
| `unreal` | textual: a class or struct definition in a header under `sdks/unreal/src` | textual: a string literal in `crates/codegen/src/unrealcpp.rs` | | `cargo ci sdk-names --check` |

Every test except Unreal's compiles or type-checks each name, so it fails if a name stops resolving. Deprecated Rust names are imported under `#[allow(deprecated)]`. A TypeScript class (`kind = "class"`) is imported as a value and used, so that its value alias must resolve too. These tests check that names resolve, and only the C++ case also asserts that each deprecated alias names the same type as its replacement. Elsewhere, the hand-written `crates/bindings-typescript/src/lib/deprecated_aliases.test-d.ts` and `deprecated_aliases_are_identical` in `crates/bindings/src/lib.rs` remain the identity checks.

CI does not compile Unreal code, so the Unreal check is textual and weaker. An SDK name must be defined, as `class` or `struct` followed by `{`, `:`, or `final`, in live code of a header under `sdks/unreal/src`: comments, string literals, and dead preprocessor branches don't count. A branch is dead if its condition is literally `0`, or if an earlier branch of the same `#if` has a condition that is literally `1`; a branch with any other condition counts as live. A generated name must appear in a string literal of `crates/codegen/src/unrealcpp.rs`, not in a comment, with `{module_prefix}` read as empty and any other `{...}` matching any placeholder, so `F{Module}Event` matches `"F{module_name}Event"`. The check does not prove that the name compiles, or that codegen still reaches the code that emits it.

The client checks of generated names use checked-in bindings, which CI regenerates and diff-checks: the Rust view client's and the C# regression client's in their own jobs, and the TypeScript test app's in `cargo ci typescript-test`, which ignores the lines that only record the CLI version.

The C++ case and the Rust view client's test are only built because other files list them: `run-compile-tests.sh` and `run-compile-tests.ps1` list the case in their `indexes` suite, and the view client's `lib.rs` declares `mod sdk_names;`. `cargo ci sdk-names` fails if one of those lines is missing or commented out, by a line comment or a block comment.

## Adding a role or a name

1. Edit `manifest.toml`: add a `[[role]]`, or change a column of an existing role. When you rename an item, move its old name to `deprecated` if the language keeps it as an alias, and record a deviation for any name that the scheme would spell differently.
2. Run `cargo ci sdk-names` to regenerate the tests, and commit the manifest and the tests together.
3. Build the affected SDKs with the commands above, or let CI do it. A name that does not resolve fails that language's test.

A new placeholder in a generated name needs a value in the column's `[instances]` table, taken from that column's test project.

## Adding a language

The proposal's recipe is:

1. For each role in the manifest, apply the vocabulary and the rendering rules to produce the language's name, adding rendering rules for the new language first.
2. Add the language's column to the manifest, recording any deviation and its reason.
3. Implement the SDK using those names.
4. Add the language's conformance test, so CI checks the names.

Here, step 1 means adding a column to the rendering rules above, and step 2 means adding the column's key to every role in `manifest.toml`, with an `[instances.<column>]` table if its generated names have placeholders. Steps 2 and 4 also mean these edits to `tools/ci/commands/sdk-names/src/main.rs`:

* add a variant to the `Column` enum and to `Column::ALL`, and its manifest key to `Column::key()`;
* add the column's field to `Role`, and its arm to `Role::cell()`;
* if the language's rendering rules add something to a name, such as a required prefix, teach `scheme_words()` to undo it, and describe it under [Name check](#name-check);
* add an `Output` to `outputs()` that writes the test into a project that CI already builds, with a writer like `rust_uses()`, `ts_imports()`, `csharp_typeofs()`, or `cpp_case()`;
* if that project only builds the test because another file lists it, add that line to `WIRING`.

Then add the column to the table in [Where each language is checked](#where-each-language-is-checked).

## Open naming questions

Some names are provisional: the stage rule fixes their suffix, but the rest of the name is still being decided. Their roles' notes say so: `module.decl.schema` and `client.decl.module`, with their `Untyped` and `Of` forms.

The vocabulary does not determine a name for these roles, so they have no `name`, and the name check skips them:

* `query.table`: Rust, C#, Unreal, and C++ say `Table`; TypeScript says `TableRef`.
* `query.col` and `query.ix_col`: Rust, C#, Unreal, and C++ say `Col` and `IxCol`; TypeScript says `ColumnExpression`, and has no indexed-column type.
* `query.bool_expr`: TypeScript says `BooleanExpr`; the others say `BoolExpr`.
* `client.reducer_status`: Rust and C# say `Status`. The proposal leaves aligning it with TypeScript's unexported `ReducerOutcome`, which has different variants, to future work.
* `module.host_error`: C#'s `StdbException` and TypeScript's `SpacetimeHostError` both misspell the product name, but the vocabulary has no term for errors, which the proposal leaves to future work, so it does not say whether the name ends in `Error` or `Exception`.
* `module.rng`: the product-name rule removes the `Stdb` of Rust's and C++'s `StdbRng`, but the vocabulary does not choose between `Rng` and TypeScript's `Random`.
