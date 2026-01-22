---
title: Next.js Quickstart
sidebar_label: Next.js
slug: /quickstarts/nextjs
hide_table_of_contents: true
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { StepByStep, Step, StepText, StepCode } from "@site/src/components/Steps";


Get a SpacetimeDB Next.js app running in under 5 minutes.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

---

<StepByStep>
  <Step title="Create your project">
    <StepText>
      Run the `spacetime dev` command to create a new project with a SpacetimeDB module and Next.js client.

      This will start the local SpacetimeDB server, publish your module, generate TypeScript bindings, and start the Next.js development server.
    </StepText>
    <StepCode>
```bash
spacetime dev --template nextjs-ts my-nextjs-app
```
    </StepCode>
  </Step>

  <Step title="Open your app">
    <StepText>
      Navigate to [http://localhost:3001](http://localhost:3001) to see your app running.

      Note: The Next.js dev server runs on port 3001 to avoid conflict with SpacetimeDB on port 3000.
    </StepText>
  </Step>

  <Step title="Explore the project structure">
    <StepText>
      Your project contains both server and client code using the Next.js App Router.

      Edit `spacetimedb/src/index.ts` to add tables and reducers. Edit `app/page.tsx` to build your UI.
    </StepText>
    <StepCode>
```
my-nextjs-app/
├── spacetimedb/          # Your SpacetimeDB module
│   └── src/
│       └── index.ts      # Server-side logic
├── app/                  # Next.js App Router
│   ├── layout.tsx        # Root layout with providers
│   ├── page.tsx          # Home page
│   └── providers.tsx     # SpacetimeDB provider (client component)
├── src/
│   └── module_bindings/  # Auto-generated types
└── package.json
```
    </StepCode>
  </Step>

  <Step title="Understand tables and reducers">
    <StepText>
      Open `spacetimedb/src/index.ts` to see the module code. The template includes a `person` table and two reducers: `add` to insert a person, and `say_hello` to greet everyone.

      Tables store your data. Reducers are functions that modify data — they're the only way to write to the database.
    </StepText>
    <StepCode>
```typescript
import { schema, table, t } from 'spacetimedb/server';

export const spacetimedb = schema(
  table(
    { name: 'person', public: true },
    {
      name: t.string(),
    }
  )
);

spacetimedb.reducer('add', { name: t.string() }, (ctx, { name }) => {
  ctx.db.person.insert({ name });
});

spacetimedb.reducer('say_hello', (ctx) => {
  for (const person of ctx.db.person.iter()) {
    console.info(`Hello, ${person.name}!`);
  }
  console.info('Hello, World!');
});
```
    </StepCode>
  </Step>

  <Step title="Test with the CLI">
    <StepText>
      Use the SpacetimeDB CLI to call reducers and query your data directly.
    </StepText>
    <StepCode>
```bash
# Call the add reducer to insert a person
spacetime call my-nextjs-app add Alice

# Query the person table
spacetime sql my-nextjs-app "SELECT * FROM person"
 name
---------
 "Alice"

# Call say_hello to greet everyone
spacetime call my-nextjs-app say_hello

# View the module logs
spacetime logs my-nextjs-app
2025-01-13T12:00:00.000000Z  INFO: Hello, Alice!
2025-01-13T12:00:00.000000Z  INFO: Hello, World!
```
    </StepCode>
  </Step>

  <Step title="Understand the provider pattern">
    <StepText>
      SpacetimeDB is client-side only — it cannot run during server-side rendering. The `app/providers.tsx` file uses the `"use client"` directive and wraps your app with `SpacetimeDBProvider`.

      The template uses environment variables for configuration. Set `NEXT_PUBLIC_SPACETIMEDB_HOST` and `NEXT_PUBLIC_SPACETIMEDB_DB_NAME` to override defaults.
    </StepText>
    <StepCode>
```tsx
// app/providers.tsx
'use client';

import { useMemo } from 'react';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection } from '../src/module_bindings';

const HOST = process.env.NEXT_PUBLIC_SPACETIMEDB_HOST ?? 'ws://localhost:3000';
const DB_NAME = process.env.NEXT_PUBLIC_SPACETIMEDB_DB_NAME ?? 'my-nextjs-app';

export function Providers({ children }: { children: React.ReactNode }) {
  const connectionBuilder = useMemo(() =>
    DbConnection.builder()
      .withUri(HOST)
      .withModuleName(DB_NAME),
    []
  );

  return (
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      {children}
    </SpacetimeDBProvider>
  );
}
```
    </StepCode>
  </Step>

  <Step title="Use React hooks for data">
    <StepText>
      In your page components, use `useTable` to subscribe to table data and `useReducer` to call reducers. All components using these hooks must have the `"use client"` directive.
    </StepText>
    <StepCode>
```tsx
// app/page.tsx
'use client';

import { tables, reducers } from '../src/module_bindings';
import { useTable, useReducer } from 'spacetimedb/react';

export default function Home() {
  // Subscribe to table data - returns [rows, isLoading]
  const [people] = useTable(tables.person);

  // Get a function to call a reducer
  const addPerson = useReducer(reducers.add);

  const handleAdd = () => {
    // Call reducer with object syntax
    addPerson({ name: 'Alice' });
  };

  return (
    <ul>
      {people.map((person, i) => <li key={i}>{person.name}</li>)}
    </ul>
  );
}
```
    </StepCode>
  </Step>
</StepByStep>

## Next steps

- See the [Chat App Tutorial](/tutorials/chat-app) for a complete example
- Read the [TypeScript SDK Reference](/sdks/typescript) for detailed API docs
