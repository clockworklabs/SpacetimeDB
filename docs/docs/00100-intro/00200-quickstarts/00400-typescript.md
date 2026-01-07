---
title: TypeScript
slug: /quickstarts/typescript
id: quickstart-typescript
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";

# TypeScript Quickstart

Get a SpacetimeDB TypeScript app running in under 5 minutes.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

## Create your project

```bash
spacetime dev --template basic-typescript my-spacetime-app
```

This command:
1. Creates a new project with a TypeScript SpacetimeDB module
2. Starts the local SpacetimeDB server
3. Publishes your module
4. Generates TypeScript client bindings

## Project structure

```
my-spacetime-app/
├── spacetimedb/          # Your SpacetimeDB module
│   └── src/
│       └── index.ts      # Server-side logic
├── client/               # Client application
│   └── src/
│       ├── index.ts
│       └── module_bindings/  # Auto-generated types
└── package.json
```

## Test your module

Call a reducer from the CLI:

```bash
spacetime call --server local my-spacetime-app your_reducer "arg1"
```

Query your data:

```bash
spacetime sql --server local my-spacetime-app "SELECT * FROM your_table"
```

## Next steps

- Edit `spacetimedb/src/index.ts` to add tables and reducers
- Build your client application using the generated bindings
- See the [Chat App Tutorial](/docs/tutorials/chat-app) for a complete example
- Read the [TypeScript SDK Reference](/sdks/typescript) for detailed API docs
