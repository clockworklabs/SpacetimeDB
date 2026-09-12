---
title: Environment Variables
slug: /databases/environment-variables
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Environment Variables

Environment variables store configuration and secrets for a database, such as API keys and deployment settings. A module declares the names it reads and any allowed values. Publishing preserves stored values unless you explicitly replace or delete them. Undeclared values can be stored before a module starts using them. Module code reads them through `ctx.env`, or `ctx.Env` in C#.

Use environment-only publishing to change configuration without uploading a module. Use a [private table](#dynamically-editable-or-untyped-secrets) when module code must edit values or read arbitrary keys.

This guide assumes a module set up using a quickstart, such as the [Rust quickstart](../../00100-intro/00200-quickstarts/00500-rust.md), and familiarity with [publishing](./00300-spacetime-publish.md).

## Declare and read variables

Declare every environment key in the module. All values are strings. A declaration can accept any string, one exact string, or a set of allowed strings. Optional declarations permit an absent value.

These examples declare a required `API_KEY`, a required `MODE` restricted to `development` or `production`, and an optional `LOG_LEVEL` restricted to `info` or `debug`.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

Pass the declaration as the `env` option to `schema`. Include the module's existing tables in the first argument if it has any.

```typescript
import { schema, t } from 'spacetimedb/server';

const spacetimedb = schema(
  {},
  {
    env: {
      API_KEY: t.string(),
      MODE: t.enum('Mode', ['development', 'production']),
      LOG_LEVEL: t.enum('LogLevel', ['info', 'debug']).optional(),
    },
  }
);

export default spacetimedb;
```

Inside a reducer, procedure, or view, read values from its context:

```typescript
const apiKey: string = ctx.env.API_KEY;
const mode: 'development' | 'production' = ctx.env.MODE;
const logLevel: 'info' | 'debug' | undefined = ctx.env.LOG_LEVEL;
const checked: string | null = ctx.env.get('LOG_LEVEL');
```

Within an environment declaration, a simple enum specifies allowed strings. Enums used elsewhere in the module retain their usual tagged representation. An enum with one case restricts the value to that string. Enums with payloads cannot be used as environment constraints.

The string-key getter checks the same declaration and permissions as named accessors. Reading an undeclared key fails. Optional named accessors return `undefined`; `ctx.env.get` returns `null` for an absent optional value.

Key names are exact and case-sensitive. The name `get` is reserved for the getter; read a declaration named `get` with `ctx.env.get('get')`. A module with no environment declarations cannot read stored environment values.

Reads inside a transaction use that transaction's snapshot. In a procedure outside a transaction, each read uses a separate snapshot. To read several keys consistently, group the reads in `withTx`.

</TabItem>
<TabItem value="rust" label="Rust">

Declare allowed strings with enums, then add one environment declaration to the module:

```rust
#[derive(spacetimedb::EnvironmentValue)]
pub enum Mode {
    #[env(value = "development")]
    Development,
    #[env(value = "production")]
    Production,
}

#[derive(spacetimedb::EnvironmentValue)]
pub enum LogLevel {
    #[env(value = "info")]
    Info,
    #[env(value = "debug")]
    Debug,
}

#[spacetimedb::env]
pub struct Env {
    pub API_KEY: String,
    pub MODE: Mode,
    pub LOG_LEVEL: Option<LogLevel>,
}
```

Inside a reducer, procedure, or view, read values from its context:

```rust
let api_key: String = ctx.env.API_KEY();
let mode: Mode = ctx.env.MODE();
let log_level: Option<LogLevel> = ctx.env.LOG_LEVEL();
let checked: Option<String> = ctx.env.get("LOG_LEVEL");
```

`String` accepts any string. An enum restricts values to its variants, and `Option<T>` permits absence. Type aliases work for these types. Without an attribute, a variant accepts its exact Rust name. An explicit mapping such as `#[env(value = "in progress")] InProgress` supports spaces, capitalization, Unicode, or the empty string. Mappings must be unique; variants cannot have payloads. A one-variant enum declares a single allowed string. These mappings affect environment reads and declarations only, not the enum's ordinary serialization.

Existing `#[env(values(...))]` constraints on `String` and `Option<String>` fields remain supported. Use enum variants to constrain typed enum fields.

The string-key getter checks the same declaration and permissions as named accessors. Reading an undeclared key fails. Optional named accessors and `ctx.env.get` return `None` for an absent optional value.

Key names are exact and case-sensitive. The name `get` is reserved for the getter; read a declaration named `get` with `ctx.env.get("get")`. A module with no environment declarations cannot read stored environment values.

Reads inside a transaction use that transaction's snapshot. In a procedure outside a transaction, each read uses a separate snapshot. To read several keys consistently, group the reads in `with_tx`.

</TabItem>
<TabItem value="csharp" label="C#">

Add one environment declaration to the module:

```csharp
#nullable enable

[SpacetimeDB.Env]
public partial struct EnvironmentSchema
{
    public string API_KEY;
    [SpacetimeDB.EnvValues("development", "production")]
    public string MODE;
    [SpacetimeDB.EnvValues("info", "debug")]
    public string? LOG_LEVEL;
}
```

Inside a reducer, procedure, or view, read values from its context:

```csharp
string apiKey = ctx.Env.API_KEY;
string mode = ctx.Env.MODE;
string? logLevel = ctx.Env.LOG_LEVEL;
string? checkedValue = ctx.Env.Get("LOG_LEVEL");
```

`string` requires a value, and `string?` permits absence. `[SpacetimeDB.EnvValues(...)]` restricts the allowed strings. Supplying one string makes it an exact-value constraint.

The string-key getter checks the same declaration and permissions as named accessors. Reading an undeclared key fails. Optional named accessors and `ctx.Env.Get` return `null` for an absent optional value.

Key names are exact and case-sensitive. Names that collide with `Get`, `ModuleEnvironment`, or inherited `Object` methods are available through the string-key getter, for example `ctx.Env.Get("GetType")`. A module with no environment declarations cannot read stored environment values.

Reads inside a transaction use that transaction's snapshot. In a procedure outside a transaction, each read uses a separate snapshot. To read several keys consistently, group the reads in `WithTx`.

</TabItem>
<TabItem value="cpp" label="C++">

Declare the environment in a dedicated header named `environment.h`:

```cpp
#pragma once
#include <spacetimedb/environment_declaration.h>

SPACETIMEDB_ENV(
    (API_KEY, std::string),
    (MODE, std::string, ("development", "production")),
    (LOG_LEVEL, std::optional<std::string>, ("info", "debug"))
)
```

In `CMakeLists.txt`, set the header path **before** adding the SpacetimeDB module library directory:

```cmake
set(SPACETIMEDB_ENV_HEADER "${CMAKE_CURRENT_SOURCE_DIR}/environment.h")
```

The library's CMake target includes this declaration consistently in the library and module source files that use the context type. Including it manually in just one source file is insufficient.

Inside a reducer or procedure, read values from its context:

```cpp
std::string api_key = ctx.env.API_KEY();
std::string mode = ctx.env.MODE();
std::optional<std::string> log_level = ctx.env.LOG_LEVEL();
std::optional<std::string> checked = ctx.env.get("LOG_LEVEL");
```

`std::string` requires a value, and `std::optional<std::string>` permits absence. The optional third element restricts the allowed strings. Supplying one string makes it an exact-value constraint.

The string-key getter checks the same declaration and permissions as named accessors. Reading an undeclared key fails. Optional named accessors and `ctx.env.get` return `std::nullopt` for an absent optional value.

Key names are exact and case-sensitive. Names reserved by the generated accessor type, including `get`, remain available through the string-key getter, for example `ctx.env.get("get")`. A module with no environment declarations cannot read stored environment values.

Reads inside a transaction use that transaction's snapshot. In a procedure outside a transaction, each read uses a separate snapshot. To read several keys consistently, group the reads in `with_tx`.

</TabItem>
</Tabs>

## Supply values when publishing

Add non-secret defaults to the selected database target in `spacetime.json`:

```json
{
  "database": "env-example",
  "server": "http://127.0.0.1:3000",
  "module-path": "./spacetimedb",
  "env": {
    "MODE": "development",
    "LOG_LEVEL": "info"
  }
}
```

With a local server running on port 3000, publish from the directory containing that configuration. This example uses a disposable development value:

```bash
API_KEY='development-only-key' spacetime publish --server http://127.0.0.1:3000
```

For an existing database, you can enter credentials once through its website settings and reuse them on later publishes. For initial publishing, supply the value through the publishing process's environment, `spacetime.local.json`, or `spacetime.{environment}.local.json`. Keep checked-in `spacetime.json` and `spacetime.{environment}.json` limited to non-secret defaults. Ensure the local files are ignored by Git:

```gitignore
spacetime.local.json
spacetime.*.local.json
```

The `.local` naming convention does not itself prevent a file from being committed.

The CLI resolves each declared key in this order:

1. A value in the publishing process's environment.
2. A value in the resolved configuration's `env` map.
3. The stored database value, unless `--replace-env` is specified.
4. Absence, which is accepted only for an optional declaration.

A shell value overrides JSON even when it is an empty string or the key is absent from JSON. Already-exported variables behave the same as inline assignments. Unrelated shell variables are ignored unless the module declares their names. The CLI displays supplied key names and their sources, without printing their values.

The configuration files `spacetime.json`, `spacetime.local.json`, `spacetime.{environment}.json`, and `spacetime.{environment}.local.json` apply in increasing precedence, where _environment_ is the environment selected with `--env`. Their `env` maps merge by key, as do maps inherited by child database targets. A higher-precedence value replaces that key while preserving unrelated keys. An empty map does not erase inherited keys.

JSON strings pass through unchanged. Booleans and numbers are converted to strings, so `false` supplies `"false"`; declarations still validate strings. Use JSON strings when exact numeric spelling matters. Arrays, objects, and `null` are rejected. Explicit JSON keys the module has not declared are accepted as stored strings; module code cannot read them. A matching shell variable is not forwarded until the key is declared. An invalid effective value rejects the publish rather than falling back to a lower-precedence value. Omitting an optional key preserves any stored value. To clear it, use `--unset-env` and remove it from effective JSON and shell inputs. An empty string is a value, not a deletion instruction.

### Preserve, edit, or replace values

After a successful publish, every required declaration has a stored value, and every present declared value satisfies its constraint. Publishing validates the resulting environment atomically before initialization or migration. You can supply a replacement value in the same publish that changes its declaration. Failure leaves the previous module and values unchanged.

An ordinary publish retains unspecified values, including values whose declarations were removed. You can explicitly supply undeclared keys in JSON or through website editing before deploying a declaration. Later publishing validates those stored strings against the new declaration. Removing a declaration does not delete its value, but module reads of that key then fail.

Update an existing database without building or uploading a module:

```bash
API_KEY='development-only-replacement' spacetime publish env-example --env-only --server http://127.0.0.1:3000
```

This uses the deployed schema for declared shell overrides and accepts explicit JSON values for undeclared keys. It does not run initialization or migration. A stale module version rejects the request; refresh and retry. For a new database, supply required values together with the initial module publish so they are available in `init`.

Delete an optional or undeclared value explicitly, after removing it from effective JSON and shell inputs:

```bash
spacetime publish env-example --env-only --unset-env LOG_LEVEL --server http://127.0.0.1:3000
```

Deleting a required value is rejected. Repeat `--unset-env` to delete multiple keys. To replace the entire store with the supplied values, use `--replace-env` with either normal publishing or `--env-only`. Replacement deletes every omitted declared and undeclared key, and fails atomically if required values are missing. It cannot be combined with `--unset-env`.

The destructive `--delete-data` operation still clears all database data, including environment values. Supply required secrets again when resetting a database. `--env-only` cannot be combined with a reset.

The same preservation rules apply to precompiled modules published with `--bin-path`. The CLI reads declarations from the artifact being published. HTTP clients and module procedures can use the [HTTP publish format](../../00300-resources/00200-reference/00200-http-api/00300-database.md#publishing-with-environment-values); project configuration and shell overrides are CLI conveniences.

## Inspect published values

The database owner and collaborators with private-table read access can inspect the environment. For the local database above:

```bash
spacetime env list env-example --server http://127.0.0.1:3000
spacetime env get env-example MODE --server http://127.0.0.1:3000
```

`env list` prints a table of keys and values. `env get` prints the requested value and fails if it is absent. Both are explicit inspection commands and include secrets in their output. Automatic publish output still omits values.

Values are stored in the private system table `st_env`. Authorized SQL reads are also supported:

```sql
SELECT key FROM st_env;
SELECT value FROM st_env WHERE key = 'MODE';
```

SQL writes to `st_env`, module-side writes, and separate CLI setters are not supported. Changes go through a validated module or environment-only publish.

## Access and limits

Publishing environment values uses the same permission as publishing the module. This includes Developer-role collaborators where they can publish modules. Environment changes add no separate Admin requirement; a destructive reset or container configuration change still requires its normal permissions.

Reducers, procedures, views, and HTTP handlers entered by the host in the root module can read its declared environment. Host-dispatched submodule entry points cannot read it, and submodules cannot declare a nonempty environment. There is no separate environment-variable namespace to configure for a submodule. A module with environment declarations can be published independently as a root module, but cannot be included as a submodule with those declarations. See [Submodules](./00600-submodules.md) for this restriction. Ordinary helper calls retain their calling entry point's access, including helpers defined in libraries or submodules. Root code can also pass a value to a helper explicitly.

A procedure suspended across a publish cannot read values belonging to a replacement program. Environment reads in views participate in dependency tracking, so publishing changed values refreshes affected views. Module code remains responsible for what it returns or logs: returning a secret from a public view exposes that value to clients.

Keys must match `[A-Za-z_][A-Za-z0-9_]*`. The limits are 256 bytes per key, 8 KiB per value, 256 declarations per database, and 256 stored values including undeclared keys. Length limits count UTF-8 bytes. Values may be empty if their declaration accepts an empty string.

## Dynamically editable or untyped secrets

Use an ordinary [private table](../00300-tables/00400-access-permissions.md) when module code must edit secrets or read keys without declaring them. For example, a table with a string primary-key column and a string value column can store arbitrary secret names and values. The table itself still has typed columns; its individual keys need no environment declarations or constraints.

Update that table through reducers that explicitly authorize the caller. Keeping a table private controls direct client reads; it does not authorize calls to a reducer that modifies or returns its contents. Apply the same care to views, procedure results, and logs. Private tables follow the database's normal private-table permissions, including administrative reads.

:::warning Secret visibility
Private tables can store dynamically editable secrets, but changing a table to public can expose its contents to clients. Environment variables have no public-table visibility setting. With either approach, module code can still expose secrets through return values or logs.
:::

Both approaches are supported. Environment declarations additionally guarantee that required values are validated and available before `init` or migration runs. Private-table values follow the table's ordinary update and migration behavior. They do not receive environment schema validation or environment publish semantics.
