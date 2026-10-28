# Module-shaped TypeScript client bindings

Implementation in progress.

- Declare client schemas through `spacetimedb/schema`, using module-style tables and body-free function declarations.
- Let `createClient` construct the typed client API from those declarations.
- Generate a thin entry point that preserves existing connection, query, descriptor, and type exports.
- Keep existing generated bindings supported and retain legacy CLI generation for older SDKs.
- Keep server module syntax and `ModuleDef` unchanged.

Implementation phases: declaration API, SDK factory, codegen, and integration.

Validation uses representative module fixtures, full generated-output snapshots, consumer type tests, runtime integration tests, and legacy compatibility tests.
