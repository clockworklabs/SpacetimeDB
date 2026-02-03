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
      Navigate to [http://localhost:3000](http://localhost:3000) to see your app running.

      The `spacetime dev` command automatically configures your app to connect to SpacetimeDB via environment variables in `.env.local`.
    </StepText>
  </Step>

  <Step title="Explore the project structure">
    <StepText>
      Your project contains both server and client code using the Next.js App Router.

      Edit `spacetimedb/src/index.ts` to add tables and reducers. Edit `app/page.tsx` and `app/PersonList.tsx` to build your UI.
    </StepText>
    <StepCode>
```
my-nextjs-app/
├── spacetimedb/          # Your SpacetimeDB module
│   └── src/
│       └── index.ts      # SpacetimeDB module logic
├── app/                  # Next.js App Router
│   ├── layout.tsx        # Root layout with providers
│   ├── page.tsx          # Server Component (fetches initial data)
│   ├── PersonList.tsx    # Client Component (real-time updates)
│   └── providers.tsx     # SpacetimeDB provider for real-time
├── lib/
│   └── spacetimedb-server.ts  # Server-side data fetching
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

  <Step title="Understand server-side rendering">
    <StepText>
      The SpacetimeDB SDK works both server-side and client-side. The template uses a hybrid approach:

      - **Server Component** (`page.tsx`): Fetches initial data during SSR for fast page loads
      - **Client Component** (`PersonList.tsx`): Maintains a real-time WebSocket connection for live updates

      The `lib/spacetimedb-server.ts` file provides a utility for server-side data fetching.
    </StepText>
    <StepCode>
```tsx
// lib/spacetimedb-server.ts
import { DbConnection } from '../src/module_bindings';

export async function fetchPeople() {
  return new Promise((resolve, reject) => {
    const connection = DbConnection.builder()
      .withUri(process.env.SPACETIMEDB_HOST!)
      .withModuleName(process.env.SPACETIMEDB_DB_NAME!)
      .onConnect(conn => {
        conn.subscriptionBuilder()
          .onApplied(() => {
            const people = Array.from(conn.db.person.iter());
            conn.disconnect();
            resolve(people);
          })
          .subscribe('SELECT * FROM person');
      })
      .build();
  });
}
```
    </StepCode>
  </Step>

  <Step title="Use React hooks for real-time data">
    <StepText>
      In client components, use `useTable` to subscribe to table data and `useReducer` to call reducers. The Server Component passes initial data as props for instant rendering.
    </StepText>
    <StepCode>
```tsx
// app/page.tsx (Server Component)
import { PersonList } from './PersonList';
import { fetchPeople } from '../lib/spacetimedb-server';

export default async function Home() {
  const initialPeople = await fetchPeople();
  return <PersonList initialPeople={initialPeople} />;
}
```

```tsx
// app/PersonList.tsx (Client Component)
'use client';

import { tables, reducers } from '../src/module_bindings';
import { useTable, useReducer } from 'spacetimedb/react';

export function PersonList({ initialPeople }) {
  // Real-time data from WebSocket subscription
  const [people, isLoading] = useTable(tables.person);
  const addPerson = useReducer(reducers.add);

  // Use server data until client is connected
  const displayPeople = isLoading ? initialPeople : people;

  return (
    <ul>
      {displayPeople.map((person, i) => <li key={i}>{person.name}</li>)}
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
