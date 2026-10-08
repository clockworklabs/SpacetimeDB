> ⚠️ **Unstable Project** ⚠️
>
> The interface of this project is **not** stable and may change without notice.

See the [C# module library reference](https://spacetimedb.com/docs/core-concepts) and the [C# client SDK reference](https://spacetimedb.com/docs/clients/c-sharp) for stable, user-facing documentation.

## Internal documentation

These projects contain the SpacetimeDB SATS typesystem, codegen and runtime bindings for SpacetimeDB WebAssembly modules. It also contains serialization code for SpacetimeDB C# clients.


The [`BSATN.Codegen`](./BSATN.Codegen/) and [`BSATN.Runtime`](./BSATN.Runtime/) libraries are used by:
- C# Modules
- and C# Client applications.

Together they provide serialization and deserialization to the BSATN format. See their READMEs for more information.

The [`Codegen`](./Codegen/) and [`Runtime`](./Runtime/) libraries are used:
- only by C# Modules.

They provide all of the functionality needed to write SpacetimeDB modules in C#. See their READMEs for more information.


### Assembly dependencies and namespaces

Assembly composition requires .NET 10 and C# 14 in both the root and its module
dependencies. Existing standalone .NET 8 modules keep their generated contexts;
they cannot participate in this assembly composition path.

Reference another C# module with a normal `ProjectReference` (or a package
containing its compiled module assembly). No root/library build-role property is
required: the same dependency can also be published independently.
Descriptor-bearing dependencies, including transitive dependencies, register
automatically in root `public` unless targeted by a namespace declaration:

```csharp
[assembly: SpacetimeDB.Namespace(typeof(AuthLib.Marker), Accessor = "MyAuth", Name = "auth_data")]
```

The marker can be any accessible type declared in the dependency assembly.
The tested example is [namespace-test-cs](../../modules/namespace-test-cs/),
whose root references AuthLib, AuditLib, and ExtraLib. If libraries live beneath
the root project directory, exclude their source files from its compile glob,
as that example does.

Root code uses `ctx.Db.MyAuth.User` and `ctx.From.MyAuth.User()`. Compiled
AuthLib helpers continue to use `ctx.Db.User` and `ctx.From.User()`, regardless
of the namespace selected by the root. Ordinary helper calls share the caller's
context and transaction; a namespace is not a security boundary between helpers.

`Accessor` controls the C# member name; `Name` controls the database namespace.
For this example, raw SQL uses `auth_data.auth_users`, while generated clients
use `conn.Db.MyAuth.User` and `q.From.MyAuth.User()`. Generated client queries and
network calls use the canonical database names automatically.
When `Name` is omitted, the host applies the containing module's case-conversion policy
to the accessor: with the default `SnakeCase` policy, `MyAuth` becomes `my_auth`.
An explicit `Name` is used as supplied, without case conversion.

#### Nested submodules

A dependency may declare its own namespace attributes. For example, if root
mounts Auth and Auth mounts Audit, root can access `ctx.Db.Auth.Audit.User`.
Mounting the same Audit assembly elsewhere creates separate tables, not an alias
for Auth's tables. Dispatch supplies each reducer, procedure, and view with its
own instance's table handles. Public contributions add no path segment.

Module query builders follow the same paths: root can use
`ctx.From.Auth.Audit.User()`, while Audit uses `ctx.From.User()`. The query
receiver carries the current instance, and resolved SQL names are cached per
table and instance, so repeated copies query their own tables.

Use `As` when passing a context to a C# method that must address a particular
copy of a library:

```csharp
Audit.Functions.Write(ctx.As.Auth.Audit);
Audit.Functions.Write(ctx.As.DirectAudit);
```

Here `Auth`, `Audit`, and `DirectAudit` are mount accessors. The called method
keeps its ordinary context parameter and library-local `ctx.Db`/`ctx.From`
expressions. Selection preserves the context type, including read-only view
contexts, and does not modify the caller. It shares sender/auth, RNG, UUID
counter, and transaction state; it does not start a transaction or invoke a host
function. Procedure and handler contexts keep their existing `WithTx` behavior.

To keep a selected context in a local variable, specify its context type, for
example `ReducerContext audit = ctx.As.Auth.Audit;`. With `var`, the value is the
generated path selector, not the context. Traversing the path uses value types;
conversion to a different instance creates a context wrapper and database
receiver. Reuse a typed local when making repeated calls to the same instance.

Generated clients expose the same full accessor paths on `Db`, `Reducers`,
`Procedures`, and `q.From`.

See [nested-namespace-test-cs](../../modules/nested-namespace-test-cs/) and its
[client regression](../../sdks/csharp/examples~/regression-tests/nested-namespaces/).

Immediate scheduling accepts a context or a selected path on .NET 10:

```csharp
Audit.Functions.VolatileNonatomicScheduleImmediateTick(ctx.As.Auth.Audit, value);
```

Inside Audit, pass `ctx` to schedule against the current instance. This works for
both reducers and procedures. The canonical function name is resolved once and
cached for that instance. Passing a path selector does not create a context
wrapper. The existing overload without a context still works when the assembly
has a single placement; it cannot choose between repeated placements.
These APIs remain experimental, best-effort, non-durable, and non-atomic with
the caller's transaction. They do not replace scheduled tables.

Coexisting versions with the same simple assembly name are not covered by the
retained integration fixture. Repeated instances of one DLL do not establish
support for loading different versions together under NativeAOT.

#### Restrictions and limitations

- Dependencies may declare their own mounts. The same assembly can appear at
  different paths, each with independent tables. Self-mounts, mount cycles,
  sibling accessor collisions, and runtime-created namespaces are unsupported.
- `Accessor = "public"` merges declarations into the containing scope, not
  necessarily the root. A public dependency's named children become children of
  that scope. Referenced modules not targeted by any mount register automatically
  in the root's public scope.
- Dependencies in `public` inherit the containing scope's case-conversion policy
  (`SnakeCase` by default). An explicitly different policy is a compilation error.
  Mount the dependency in a named namespace to keep its independent naming policy.
- `Accessor` must be a valid C# and database identifier; optional `Name` must be
  a valid database identifier. Both are limited to 63 UTF-8 bytes.
  Accessors are checked for case-insensitive duplicates; the host validates
  canonical namespace collisions.
  `st`, `spacetimedb`, and names starting with `pg_` are reserved.
  Keywords use their plain spelling in the attribute and `@` in C# expressions.
  The `public` scope cannot be renamed or targeted through a different accessor.
- Named namespaces cannot declare lifecycle reducers, nonempty environment
  schemas, or RLS filters. The generator rejects these when composing the root.
  This applies at every depth, including public contributions inside a named
  submodule. They remain valid when published alone or registered in the root's
  public scope, subject to ordinary host validation (including lifecycle uniqueness).
- Define RLS in the root, using canonical qualified table names, as in
  [the integration fixture](../../modules/namespace-test-cs/Lib.cs).
  Library-defined RLS in a named namespace is unsupported; it must not be relied
  on to protect data.
- HTTP routes declared in named submodules are ignored, with a warning in the
  module log during publication. Define routes in the root instead; root handlers
  may call dependency helpers. Dependencies registered in `public` still contribute
  HTTP routes, using root-level routing and environment authority.
- Cross-language module composition is not currently supported.

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

Environment declarations register through each assembly's descriptor on .NET 10.
Dependencies automatically registered in `public` contribute to the root schema;
mounted dependencies cannot declare environment variables (the generator and host reject them).
A library helper invoked by root code can use `ctx.Env.Get("KEY")` for a root-declared
key. Calling that library through a namespaced host entrypoint does not grant
environment access, even for a key declared by the root.
