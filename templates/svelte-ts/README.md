Get a SpacetimeDB Svelte app running in under 5 minutes.

## Prerequisites

- A recent version of [Node.js](https://nodejs.org/) 22, 24, or 26 or later
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

Install the [SpacetimeDB CLI](https://spacetimedb.com/install) before continuing.

---

## Create your project

Run the `spacetime dev` command to create a new project with a SpacetimeDB module and Svelte client.

This will start the local SpacetimeDB server, publish your module, generate TypeScript bindings, and start the Svelte development server.

```bash
spacetime dev --template svelte-ts
```



## Open your app

Navigate to [http://localhost:5173](http://localhost:5173) to see your app running.

The template includes a basic Svelte app connected to SpacetimeDB.



## Explore the project structure

Your project contains both server and client code.

Edit `spacetimedb/src/index.ts` to add tables and reducers. Edit `src/App.svelte` to build your UI. The connection to SpacetimeDB is configured in `src/Root.svelte`.

```
my-spacetime-app/
├── spacetimedb/            # Your SpacetimeDB module
│   └── src/
│       └── index.ts        # Server-side logic
├── src/                    # Svelte frontend
│   ├── main.ts             # Mounts Root.svelte
│   ├── Root.svelte         # Connects to SpacetimeDB
│   ├── App.svelte          # Your UI
│   └── module_bindings/    # Auto-generated types
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



## Use your data in Svelte

`src/Root.svelte` calls `createSpacetimeDBProvider`, which shares the connection with every component below it. It has to run inside a component, because it uses Svelte context, so it can't move to `main.ts`.

`src/App.svelte` then reads that connection with the hooks from `spacetimedb/svelte`. `useSpacetimeDB` and `useTable` return Svelte stores, so read them with a `$` prefix. `useTable` subscribes to the table and keeps its rows up to date. `useReducer` returns a function that calls the reducer.

```svelte
<script lang="ts">
import { useSpacetimeDB, useTable, useReducer } from 'spacetimedb/svelte';
import { tables, reducers } from './module_bindings';

const conn = useSpacetimeDB();
const [people] = useTable(tables.person);
const addReducer = useReducer(reducers.add);
</script>

<p>{$conn.isActive ? 'Connected' : 'Disconnected'}</p>
<p>{$people.length} people</p>
<button onclick={() => addReducer({ name: 'Alice' })}>Add Alice</button>
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

## Next steps

- Read the [Svelte section of the TypeScript SDK Reference](https://spacetimedb.com/docs/clients/typescript#svelte) for the full Svelte API
- Work through the [Chat App Tutorial](https://spacetimedb.com/docs/tutorials/chat-app) to build a bigger module. Its client is written in React, but the module and the SDK calls carry over to Svelte.
