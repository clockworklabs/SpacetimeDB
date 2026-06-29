Get a SpacetimeDB Remix app running in under 5 minutes.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

Install the [SpacetimeDB CLI](https://spacetimedb.com/install) before continuing.

---

## Create your project

Run the `spacetime dev` command to create a new project with a SpacetimeDB module and Remix client.

This will start the local SpacetimeDB server, publish your module, generate TypeScript bindings, and start the Remix development server.

```bash
spacetime dev --template remix-ts
```



## Open your app

Navigate to [http://localhost:5173](http://localhost:5173) to see your app running.

The `spacetime dev` command automatically configures your app to connect to SpacetimeDB via environment variables.



## Explore the project structure

Your project contains both server and client code using Remix with Vite.

Edit `spacetimedb/src/index.ts` to add tables and reducers. Edit `app/routes/_index.tsx` to build your UI.

```
my-remix-app/
├── spacetimedb/          # Your SpacetimeDB module
│   └── src/
│       └── index.ts      # SpacetimeDB module logic
├── app/                  # Remix app
│   ├── root.tsx          # Root layout with SpacetimeDB provider
│   ├── lib/
│   │   └── spacetimedb.server.ts  # Server-side data fetching
│   └── routes/
│       └── _index.tsx    # Home page with loader
├── src/
│   └── module_bindings/  # Auto-generated types
└── package.json
```



## Understand tables and reducers

Open `spacetimedb/src/index.ts` to see the module code. The template includes a `person` table and two reducers: `add` to insert a person, and `sayHello` to greet everyone.

Tables store your data. Reducers are functions that modify data — they're the only way to write to the database.

```typescript
import { schema, table, t } from 'spacetimedb/server';

const spacetimedb = schema({
  person: table(
    { public: true },
    {
      name: t.string(),
    }
  ),
});
export default spacetimedb;

export const add = spacetimedb.reducer(
  { name: t.string() },
  (ctx, { name }) => {
    ctx.db.person.insert({ name });
  }
);

export const sayHello = spacetimedb.reducer(ctx => {
  for (const person of ctx.db.person.iter()) {
    console.info(`Hello, ${person.name}!`);
  }
  console.info('Hello, World!');
});
```



## Test with the CLI

Open a new terminal and navigate to your project directory. Then use the SpacetimeDB CLI to call reducers and query your data directly.

```bash
cd my-spacetime-app

# Call the add reducer to insert a person
spacetime call add Alice

# Query the person table
spacetime sql "SELECT * FROM person"
 name
---------
 "Alice"

# Call sayHello to greet everyone
spacetime call say_hello

# View the module logs
spacetime logs
2025-01-13T12:00:00.000000Z  INFO: Hello, Alice!
2025-01-13T12:00:00.000000Z  INFO: Hello, World!
```



## Understand server-side rendering

The SpacetimeDB SDK works both server-side and client-side. The template uses Remix loaders for SSR:

- **Loader**: Fetches initial data from SpacetimeDB during server rendering
- **Client**: Maintains a real-time WebSocket connection for live updates

The `app/lib/spacetimedb.server.ts` file provides a utility for server-side data fetching.

```tsx
// app/lib/spacetimedb.server.ts
import { DbConnection, tables } from '../../src/module_bindings';

export async function fetchPeople() {
  return new Promise((resolve, reject) => {
    const connection = DbConnection.builder()
      .withUri(process.env.SPACETIMEDB_HOST!)
      .withDatabaseName(process.env.SPACETIMEDB_DB_NAME!)
      .onConnect(conn => {
        conn.subscriptionBuilder()
          .onApplied(() => {
            const people = Array.from(conn.db.person.iter());
            conn.disconnect();
            resolve(people);
          })
          .subscribe(tables.person);
      })
      .build();
  });
}
```



## Use loaders and hooks for data

Use Remix loaders to fetch initial data server-side, then React hooks for real-time updates. The loader data is passed to the component and displayed immediately while the client connects.

```tsx
// app/routes/_index.tsx
import { useLoaderData } from '@remix-run/react';
import { tables, reducers } from '../../src/module_bindings';
import { useTable, useReducer } from 'spacetimedb/react';
import { fetchPeople } from '../lib/spacetimedb.server';

export async function loader() {
  const people = await fetchPeople();
  return { initialPeople: people };
}

export default function Index() {
  const { initialPeople } = useLoaderData<typeof loader>();

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

## Next steps

- See the [Chat App Tutorial](https://spacetimedb.com/docs/intro/tutorials/chat-app) for a complete example
- Read the [TypeScript SDK Reference](https://spacetimedb.com/docs/intro/core-concepts/clients/typescript-reference) for detailed API docs
