# Module-shaped TypeScript client bindings

`spacetime generate --lang typescript` emits module declarations and a thin client entry point. The SDK constructs the client implementation through `createClient`.

## Using generated bindings

**User code — unchanged**

```ts
import { DbConnection, tables, reducers } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('example')
  .build();
```

Connection classes, query builders, function descriptors, and public types retain their existing exports. Server module syntax and `ModuleDef` are unchanged.

## Declarations

- `spacetimedb/schema` exposes module-style tables and body-free reducer, procedure, and view declarations.
- `createClient` reads the default schema export and named function exports, preserving types and namespace names.
- The generated entry point calls the factory and re-exports its API. Users can also call the factory directly with handwritten declarations.
- Tables retain defaults, auto-increment, indexes, visibility, and event metadata. Views retain parameters, cardinality, and primary keys.
- Scheduling relationships remain declaration metadata; clients do not execute scheduled functions.

## Upgrading

- New-format bindings require SDK 2.13.0 or later, exposing `spacetimedb/schema` and `createClient`. Upgrade the SDK before regenerating.
- Existing bindings continue working with the new SDK; regeneration is optional.
- Use `--bindings-format legacy` to generate the previous format for older SDKs.
- The legacy generator is tested against published SDK 2.12.0 using its supported compiler settings. That SDK's existing `exactOptionalPropertyTypes` limitation remains.

## Tests

- `pnpm test:declarations`: declaration metadata, consumer types, and client runtime behavior.
- After `pnpm build`, `pnpm test:declarations:package`: packaged ESM, CommonJS, browser, and consumer-type coverage.
- Codegen tests snapshot representative modules and compile/run their generated clients in both formats.
- The test-app's saved legacy bindings remain compatibility fixtures. React and chat templates use the new format.
