> ⚠️ **Unstable Project** ⚠️
>
> The interface of this project is **not** stable and may change without notice.

See the [C# module library reference](https://spacetimedb.com/docs/modules/c-sharp) and the [C# client SDK reference](https://spacetimedb.com/docs/sdks/c-sharp) for stable, user-facing documentation.

## Internal documentation

These projects contain the SpacetimeDB SATS typesystem, codegen and runtime bindings for SpacetimeDB WebAssembly modules. It also contains serialization code for SpacetimeDB C# clients.


The [`BSATN.Codegen`](./BSATN.Codegen/) and [`BSATN.Runtime`](./BSATN.Runtime/) libraries are used by:
- C# Modules
- and C# Client applications.

Together they provide serialization and deserialization to the BSATN format. See their READMEs for more information.

The [`Codegen`](./Codegen/) and [`Runtime`](./Runtime/) libraries are used:
- only by C# Modules.

They provide all of the functionality needed to write SpacetimeDB modules in C#. See their READMEs for more information.


### Declared environment

A module may declare one `[SpacetimeDB.Env]` struct. `string` is required and
`string?` is optional. An optional `EnvValues` attribute restricts the allowed
strings; one value is a literal constraint. Values are supplied on publish, never
in module source:

```csharp
[SpacetimeDB.Env]
public partial struct EnvironmentSchema
{
    public string API_URL;
    [SpacetimeDB.EnvValues("prod", "dev")]
    public string MODE;
    public string? LOG_LEVEL;
}
```

Context access is read-only: `ctx.Env.MODE` returns `string`, while
`ctx.Env.LOG_LEVEL` returns `string?`. `ctx.Env.Get("MODE")` uses the same checked
host read. A key named `Get` keeps generic access rather than replacing the method;
C# keywords are escaped, for example `ctx.Env.@class`. Empty or absent declarations
allow no environment keys. Undeclared reads and reads from host-dispatched
submodules fail at runtime. Values are private, durable database configuration for secrets and other settings.
Database owners and authorized collaborators can read them; module code can expose
them through its own outputs.
