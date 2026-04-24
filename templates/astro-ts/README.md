Get a SpacetimeDB Astro app running in under 5 minutes.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

Install the [SpacetimeDB CLI](https://spacetimedb.com/install) before continuing.

---

## Create your project

Run the `spacetime dev` command to create a new project with a SpacetimeDB module and Astro client.

This will start the local SpacetimeDB server, publish your module, generate TypeScript bindings, and start the Astro development server.

```bash
spacetime dev --template astro-ts
```

## Open your app

Navigate to [http://localhost:4321](http://localhost:4321) to see your app running.

The Astro app reads `SPACETIMEDB_*` variables on the server and `PUBLIC_SPACETIMEDB_*` variables in the client, so `.env.local` can configure both sides of the app.

## Explore the project structure

Your project contains both server and client code using Astro SSR and a live interactive client for real-time updates.

Edit `spacetimedb/src/index.ts` to add tables and reducers. Edit `src/pages/index.astro` and `src/components/PersonList.tsx` to build your UI.

```text
my-astro-app/
├── spacetimedb/              # Your SpacetimeDB module
│   └── src/
│       └── index.ts          # SpacetimeDB module logic
├── src/
│   ├── components/
│   │   ├── PersonList.tsx
│   │   ├── SpacetimeApp.tsx
│   │   └── DeferredPeopleSnapshot.astro
│   ├── lib/
│   │   └── spacetimedb-server.ts
│   ├── module_bindings/      # Auto-generated types
│   ├── layouts/
│   │   └── Layout.astro
│   ├── pages/
│   │   └── index.astro
│   └── styles/
│       └── global.css
└── package.json
```

## Understand tables and reducers

Open `spacetimedb/src/index.ts` to see the module code. The template includes a `person` table and two reducers: `add` to insert a person, and `sayHello` to greet everyone.

Tables store your data. Reducers are functions that modify data and are the only way to write to the database.

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

## Understand Astro SSR plus real-time hydration

The template uses a hybrid rendering model:

- `src/pages/index.astro` fetches the initial list of people on the server for a fast first paint.
- `src/components/SpacetimeApp.tsx` hydrates with `client:load` and provides the SpacetimeDB connection.
- `src/components/PersonList.tsx` subscribes to the `person` table with `useTable()` and calls reducers with `useReducer()`.

This gives you server-rendered HTML on the first request and a live WebSocket-backed UI after hydration.

## Understand Astro server islands

The template also includes a deferred Astro-only section to demonstrate `server:defer`.

- `src/components/DeferredPeopleSnapshot.astro` fetches its own server-rendered snapshot.
- `src/pages/index.astro` renders it with `server:defer`, so it loads after the main page shell.

This demonstrates an Astro-specific pattern without affecting the main real-time client flow.

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

- See the [Chat App Tutorial](https://spacetimedb.com/docs/intro/tutorials/chat-app) for a complete example
- Read the [TypeScript SDK Reference](https://spacetimedb.com/docs/intro/core-concepts/clients/typescript-reference) for detailed API docs
