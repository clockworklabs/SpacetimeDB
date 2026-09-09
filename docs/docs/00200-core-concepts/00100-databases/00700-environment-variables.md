---
title: Environment Variables
slug: /databases/environment-variables
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# Environment Variables

Environment variables store configuration and secrets for a database, such as API keys and deployment settings. A module declares the names it accepts and any allowed values. Each publish supplies the complete set of values for that module. Module code reads them through `ctx.env`, or `ctx.Env` in C#.

Use environment variables for configuration that changes when a module is published. For secrets or configuration that must change without publishing, use a [private table](#dynamically-editable-or-untyped-secrets).

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

</TabItem>
</Tabs>

The string-key getter checks the same declaration and permissions as named accessors. Reading an undeclared key fails; it does not return an absent value. Named optional accessors return `None`, `undefined`, `null`, or `std::nullopt`, depending on the language. The TypeScript string-key getter uses `null` for absence.

Key names are exact and case-sensitive. The getter name `get`, or `Get` in C#, is reserved: a key with that name remains accessible through the string-key getter. A module with no environment declarations accepts no keys.

Reads inside a transaction use that transaction's snapshot. In a procedure outside a transaction, each read uses a separate snapshot. To read several keys consistently, group the reads in `with_tx` in Rust or C++, `withTx` in TypeScript, or `WithTx` in C#.

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
API_KEY='development-only-key' spacetime publish
```

For real credentials, supply the value through the publishing process's environment or an appropriately ignored local configuration file. Keep secrets out of checked-in configuration and module source.

The CLI resolves each declared key in this order:

1. A value in the publishing process's environment.
2. A value in the resolved configuration's `env` map.
3. Absence, which is accepted only for an optional declaration.

A shell value overrides JSON even when it is an empty string or the key is absent from JSON. Already-exported variables behave the same as inline assignments. Unrelated shell variables are ignored unless the module declares their names. The CLI displays supplied key names and their sources, without printing their values.

The configuration files `spacetime.json`, `spacetime.local.json`, `spacetime.{environment}.json`, and `spacetime.{environment}.local.json` apply in increasing precedence, where _environment_ is the environment selected with `--env`. Their `env` maps merge by key, as do maps inherited by child database targets. A higher-precedence value replaces that key while preserving unrelated keys. An empty map does not erase inherited keys.

JSON strings pass through unchanged. Booleans and numbers are converted to strings, so `false` supplies `"false"`; declarations still validate strings. Use JSON strings when exact numeric spelling matters. Arrays, objects, and `null` are rejected, as are JSON keys the module has not declared. An invalid effective value rejects the publish rather than falling back to a lower-precedence value.

### Every publish replaces the complete environment

Publishing validates the supplied values and installs them atomically with the module, before initialization or migration. Invalid environment configuration leaves the previous module and values unchanged. Changing only values still requires publishing, and does not rerun `init` on an existing database.

Previously stored values are **not** defaults for the next publish. Every publish must supply each required value again. An optional value omitted from all effective inputs is removed. To remove `LOG_LEVEL` in the example, remove it from every applicable configuration layer and unset any exported `LOG_LEVEL` before publishing. An empty string is a value, not a deletion instruction.

The same rules apply to precompiled modules published with `--bin-path`. The CLI reads declarations from the artifact being published.

Managed publications also retain the complete resolved environment in the operation's private local `submission.json` file. Keep the publication directory private and out of source control. `--resume-publication` sends the original request bytes, including the original values, even if project files or shell variables have changed. Missing or altered retained input causes an error; resuming never substitutes an empty environment. When preserving the current module, the CLI checks its environment declarations against authenticated metadata for the selected database and program before creating this input.

For publishing from an HTTP client or a module procedure, see the [HTTP publish format and example](../../00300-resources/00200-reference/00200-http-api/00300-database.md#publishing-with-environment-values). Supply the module and complete environment in the request body; project configuration and shell overrides are CLI conveniences.

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

SQL writes to `st_env`, module-side writes, and separate CLI setters are not supported. Changes go through a publish with the complete desired environment.

## Access and limits

Reducers, procedures, views, and HTTP handlers entered by the host in the root module can read its declared environment. Host-dispatched submodule entry points cannot read it, and submodules cannot declare a nonempty environment. Ordinary helper calls retain their calling entry point's access, including helpers defined in libraries or submodules. Root code can also pass a value to a helper explicitly.

A procedure suspended across a publish cannot read values belonging to a replacement program. Environment reads in views participate in dependency tracking, so publishing changed values refreshes affected views. Module code remains responsible for what it returns or logs: returning a secret from a public view exposes that value to clients.

Keys must match `[A-Za-z_][A-Za-z0-9_]*`. The limits are 256 bytes per key, 8 KiB per value, and 256 declarations per database. Length limits count UTF-8 bytes. Values may be empty if their declaration accepts an empty string.

## Dynamically editable or untyped secrets

Use an ordinary [private table](../00300-tables/00400-access-permissions.md) when a secret must change without republishing, or when its keys and allowed values should not be declared in the environment schema. For example, a table with a string primary-key column and a string value column can store arbitrary secret names and values. The table itself still has typed columns; its individual keys need no environment declarations or constraints.

Update that table through reducers that explicitly authorize the caller. Keeping a table private controls direct client reads; it does not authorize calls to a reducer that modifies or returns its contents. Apply the same care to views, procedure results, and logs. Private tables follow the database's normal private-table permissions, including administrative reads.

Unlike environment variables, these values follow the table's ordinary update and migration behavior. They do not receive environment schema validation or complete replacement on every publish.
