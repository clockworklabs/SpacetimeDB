---
title: spacetime publish
slug: /databases/building-publishing
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# `spacetime publish`

This guide covers how to build and publish your SpacetimeDB module.

## Building Modules

Before you can publish a module to SpacetimeDB, you need to build it for the appropriate runtime:

- **Rust and C#** modules compile to WebAssembly (WASM)
- **TypeScript** modules bundle for V8 JavaScript engine

Navigate to your module directory (typically `spacetimedb/` within your project) and run:

```bash
spacetime build
```

This compiles your module and validates its structure.

:::tip
If you're publishing your module, you don't need to run `spacetime build` separately - `spacetime publish` will automatically build your module if needed.
:::

For all build options, see the [`spacetime build` CLI reference](/cli-reference#spacetime-build).

## Publishing Modules

Once you've built your module, you can publish it to create a new database or update an existing one.

### Prerequisites

Before publishing, authenticate with SpacetimeDB:

```bash
spacetime login
```

This opens a browser window for authentication. Once complete, your credentials are saved locally.

### Publishing a New Database

To publish your module and create a new database:

```bash
spacetime publish <DATABASE_NAME>
```

This command:

1. Builds your module (if not already built)
2. Creates a new database with the specified name
3. Uploads and installs your module
4. Runs the `init` lifecycle reducer (if defined)
5. Starts accepting client connections

After publishing, SpacetimeDB outputs your database identity. **Save this identity** - you'll need it for administrative tasks.

### Updating an Existing Database

To update a module that's already published:

```bash
spacetime publish <DATABASE_NAME>
```

SpacetimeDB will:

1. Build your module
2. Attempt to automatically migrate the schema
3. Swap in the new module atomically
4. Maintain active client connections

#### Breaking Changes

If your update includes breaking changes that cannot be automatically migrated:

```bash
spacetime publish --break-clients <DATABASE_NAME>
```

⚠️ **Warning:** This will break existing clients that haven't been updated to match your new schema.

#### Clearing Data

To completely reset your database and delete all data:

```bash
spacetime publish --delete-data <DATABASE_NAME>
```

⚠️ **Warning:** This permanently deletes all data in your database!

### Publishing Options

For all available publishing options and flags, see the [`spacetime publish` CLI reference](/cli-reference#spacetime-publish).

## Next Steps

After publishing:

- Learn about [connecting a client](/sdks) to your database
- Learn about [Tables](/tables), [Reducers](/functions/reducers), and [Procedures](/functions/procedures)
