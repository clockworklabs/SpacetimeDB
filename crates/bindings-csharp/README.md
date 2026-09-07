> ⚠️ **Unstable Project** ⚠️
>
> The interface of this project is **not** stable and may change without notice.

See the [C# module library reference](https://spacetimedb.com/docs/modules/c-sharp) and the [C# client SDK reference](https://spacetimedb.com/docs/sdks/c-sharp) for stable, user-facing documentation.

## Internal documentation

### Function visibility and invocation authentication

Reducers and procedures can declare `Visibility = FunctionVisibility.Public`,
`Private`, or `Internal` in their attributes. Omission (`Default`) means public
for ordinary functions and private for scheduled functions. An explicit choice
is preserved when the function is scheduled. Lifecycle reducers permit only
omission or `Internal` and can only run for their host lifecycle event.

Internal functions require verified internal authority. Private functions also
admit the owner, and public functions admit any client. For example:

```csharp
[Reducer(Visibility = FunctionVisibility.Internal)]
public static void ProcessJobs(ReducerContext ctx) { }
```

`ctx.SenderAuth.IsInternal` comes from the host's invocation authority. It is
independent of connection and JWT presence, so an internal call can have a JWT.
JWT identity is the verified sender supplied by the host. Newly compiled modules
emit schema V11 and advertise `hosted_auth_v1`, requiring a compatible host.

These projects contain the SpacetimeDB SATS typesystem, codegen and runtime bindings for SpacetimeDB WebAssembly modules. It also contains serialization code for SpacetimeDB C# clients.


The [`BSATN.Codegen`](./BSATN.Codegen/) and [`BSATN.Runtime`](./BSATN.Runtime/) libraries are used by:
- C# Modules
- and C# Client applications.

Together they provide serialization and deserialization to the BSATN format. See their READMEs for more information.

The [`Codegen`](./Codegen/) and [`Runtime`](./Runtime/) libraries are used:
- only by C# Modules.

They provide all of the functionality needed to write SpacetimeDB modules in C#. See their READMEs for more information.
