# Getting started

Use this guide when you want to run an example or add one of these packages to
an existing SpacetimeDB TypeScript module. Repository contributors should use
the development commands in [Components](./COMPONENTS.md#repository-development).

## Prerequisites

- Node.js 20 or later.
- npm, or pnpm 10 when running this repository's examples.
- The official SpacetimeDB launcher with CLI version 2.8.3 selected.

Install the launcher using the
[official SpacetimeDB installation guide](https://spacetimedb.com/docs/), then
select the release used by these packages:

```bash
spacetime version install 2.8.3
spacetime version use 2.8.3
spacetime --version
```

For local development, start the standalone server in a separate terminal. It
runs in the foreground on port `3000` by default:

```bash
spacetime start
```

In another terminal, verify the server and authenticate the identity that will
publish the module:

```bash
spacetime server ping local
spacetime login
spacetime login show
```

## Run an example

Each application under `<package>/example` has its own credentials, port, and
first-use instructions. The common workflow is:

```bash
cd <package>/example
pnpm install
pnpm --dir spacetimedb install
pnpm run build:module:fresh
pnpm run dev
```

Every example stores its host module in `spacetimedb/`. A few examples are pnpm
workspaces that install both projects together. Follow the exact commands in
each example README.

`build:module:fresh` deletes and recreates only that example's local database.
After the first run, use `build:module` when you want to preserve its rows.

The checked-in examples use pnpm workspace dependencies so each example tests
the component source and SDK in this repository. Consumer projects install
published packages from npm.

## Add a package to an application

### 1. Install compatible releases

Install the component in the directory that contains your SpacetimeDB module's
`package.json`. Install the TypeScript SDK as an explicit compatible peer:

```bash
npm install @spacetimedb/<package> spacetimedb@^2.8.3
```

The package README lists any companion components that must be installed too.
Use one package manager consistently in your application. The commands below
use npm; pnpm and compatible clients can install the same package versions.

### 2. Choose the integration shape

Packages in this repository have one of three shapes:

- **Mountable submodule:** import `@spacetimedb/<package>/submodule`, add it
  to `schema({ ... })`, and call its installer from the host `init` hook.
- **Factory:** construct tables and operations with application-specific typed
  handlers, then register the returned pieces in the host schema.
- **Helper:** call its pure or context-aware functions from tables, reducers,
  procedures, or handlers owned by the application.

The package README identifies the shape and provides the package-specific code.
For a mountable submodule, the basic host structure is:

```ts
import { schema } from 'spacetimedb/server';
import * as component from '@spacetimedb/<package>/submodule';

const spacetimedb = schema({ component });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  component.installComponent(ctx.as.component);
});
```

`component` and `installComponent` are placeholders. Use the exact namespace
and installer from the package README. The host owns its lifecycle hook.

### 3. Add the application boundary

A reusable component cannot decide who your users are or which browser actions
are safe. Before exposing it to a client:

1. Map `ctx.sender` or your authentication session to an application subject.
2. Wrap component helpers in narrow host reducers or procedures.
3. Expose private component state through caller- or tenant-scoped host views.
4. Register only the HTTP routes your application needs.
5. Store provider credentials in private module state, never browser code or a
   public table.

The full examples show these boundaries. Reuse the boundary pattern and select
the product-specific tables or development helpers that fit your application.

### 4. Publish and generate bindings

From the application root, replace the placeholder paths with your layout:

```bash
spacetime publish --server local --yes --module-path ./spacetimedb my-app
spacetime generate --lang typescript --out-dir ./src/module_bindings --module-path ./spacetimedb --yes
```

Publishing already builds the module. Use `--delete-data=always` only when you
intend to destroy the target database's data.

### 5. Connect the client

Import the generated connection into the browser. Server component code stays
inside the module:

```ts
import { DbConnection, tables } from './module_bindings';

const connection = DbConnection.builder()
  .withUri('ws://127.0.0.1:3000')
  .withDatabaseName('my-app')
  .onConnect(ctx => {
    ctx.subscriptionBuilder().subscribe([tables.myApplicationView]);
  })
  .build();
```

The exact generated reducer, procedure, view, and table names come from the host
module you published. Generate bindings again whenever that public schema
changes.

## Production checklist

Before deploying an integration:

- Replace example development servers and console mailers with production
  infrastructure.
- Provision service identities and secrets explicitly at deployment time.
- Use TLS, secure cookies, host validation, request limits, and trusted proxy
  configuration at the deployment boundary.
- Keep base tables private and verify that every public view is scoped to its
  caller or tenant.
- Treat provider side effects as at-least-once and use idempotency keys where
  supported.
- Test data-preserving upgrades before publishing over production data.

For the underlying platform workflow, see the official
[TypeScript quickstart](https://spacetimedb.com/docs/quickstarts/typescript/),
[publishing guide](https://spacetimedb.com/docs/databases/building-publishing/),
and [client-binding guide](https://spacetimedb.com/docs/clients/codegen/).
