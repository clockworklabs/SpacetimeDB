> ⚠️ **Unstable Project** ⚠️
>
> The interface of this project is **not** stable and may change without notice.

See the [C# module library reference](https://spacetimedb.com/docs/modules/c-sharp) and the [C# client SDK reference](https://spacetimedb.com/docs/sdks/c-sharp) for stable, user-facing documentation.

## Internal documentation

These projects contain the SpacetimeDB SATS typesystem, codegen and runtime bindings for SpacetimeDB WebAssembly modules. It also contains serialization code for SpacetimeDB C# clients.

See the 

The [`BSATN.Codegen`](./BSATN.Codegen/) and [`BSATN.Runtime`](./BSATN.Runtime/) libraries are used by:
- C# Modules
- and C# Client applications.

Together they provide serialization and deserialization to the BSATN format. See their READMEs for more information.

The [`Codegen`](./Codegen/) and [`Runtime`](./Runtime/) libraries are used:
- only by C# Modules.

They provide all of the functionality needed to write SpacetimeDB modules in C#. See their READMEs for more information.

