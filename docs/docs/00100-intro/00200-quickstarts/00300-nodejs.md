---
title: Node.js Quickstart
sidebar_label: Node.js
slug: /quickstarts/nodejs
hide_table_of_contents: true
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { StepByStep, Step, StepText, StepCode } from "@site/src/components/Steps";

Get a SpacetimeDB Node.js app running in under 5 minutes.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

---

<StepByStep>
  <Step title="Create your project">
    <StepText>
      Run the `spacetime dev` command to create a new project with a SpacetimeDB module and Node.js client.

      This starts the local SpacetimeDB server, publishes your module, generates TypeScript bindings, and runs the Node.js client.
    </StepText>
    <StepCode>

```bash
spacetime dev --template nodejs-ts
```

    </StepCode>

  </Step>

  <Step title="Explore the project structure">
    <StepText>
      Your project contains both server and client code.

      Edit `spacetimedb/src/index.ts` to add tables and reducers. Edit `src/main.ts` to build your Node.js client.
    </StepText>
    <StepCode>

```
my-spacetime-app/
├── spacetimedb/          # Your SpacetimeDB module
│   └── src/
│       └── index.ts      # Server-side logic
├── src/
│   ├── main.ts           # Node.js client script
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
    { public: true },
    {
      name: t.string(),
    }
  )
);

spacetimedb.reducer('add', { name: t.string() }, (ctx, { name }) => {
  ctx.db.person.insert({ name });
});

spacetimedb.reducer('say_hello', ctx => {
  for (const person of ctx.db.person.iter()) {
    console.info(`Hello, ${person.name}!`);
  }
  console.info('Hello, World!');
});
```

    </StepCode>

  </Step>

  <Step title="Run the client">
    <StepText>
      `spacetime dev` starts both the server and the Node.js client. The client connects to SpacetimeDB, subscribes to tables, and displays people as they are added or removed. Press Ctrl+C to exit.
    </StepText>
    <StepCode>
```bash
spacetime dev --template nodejs-ts
```
    </StepCode>
  </Step>

  <Step title="Call reducers from the SpacetimeDB CLI">
    <StepText>
      Use the SpacetimeDB CLI to add people and invoke reducers. Changes appear in your Node.js client in real time.
    </StepText>
    <StepCode>
```bash
# Add a person
spacetime call nodejs-ts add Alice
spacetime call nodejs-ts add Bob

# Greet everyone (check server logs)

spacetime call nodejs-ts say_hello

# Query the database

spacetime sql nodejs-ts "SELECT * FROM person"

````
    </StepCode>
  </Step>

  <Step title="Understand the client code">
    <StepText>
      Open `src/main.ts` to see the Node.js client. It uses `DbConnection.builder()` to connect to SpacetimeDB, subscribes to tables, and registers callbacks for insert/delete events. Unlike browser apps, Node.js stores the authentication token in a file instead of localStorage.
    </StepText>
    <StepCode>
```typescript
import { DbConnection } from './module_bindings/index.js';

DbConnection.builder()
  .withUri(HOST)
  .withDatabaseName(DB_NAME)
  .withToken(loadToken())  // Load saved token from file
  .onConnect((conn, identity, token) => {
    console.log('Connected! Identity:', identity.toHexString());
    saveToken(token);  // Save token for future connections

    // Subscribe to all tables
    conn.subscriptionBuilder()
      .onApplied((ctx) => {
        // Show current people
        const people = [...ctx.db.person.iter()];
        console.log('Current people:', people.length);
      })
      .subscribeToAllTables();

    // Listen for table changes
    conn.db.person.onInsert((ctx, person) => {
      console.log(`[Added] ${person.name}`);
    });
  })
  .build();
````

    </StepCode>

  </Step>

  <Step title="More CLI examples">
    <StepText>
      The SpacetimeDB CLI can call reducers and query your data. Changes appear in your Node.js client in real time.
    </StepText>
    <StepCode>
```bash
# Call the add reducer to insert a person
spacetime call nodejs-ts add Charlie

# Query the person table

spacetime sql nodejs-ts "SELECT * FROM person"
name

---

"Alice"
"Bob"
"Charlie"

# Call say_hello to greet everyone

spacetime call nodejs-ts say_hello

# View the module logs

spacetime logs
2025-01-13T12:00:00.000000Z INFO: Hello, Alice!
2025-01-13T12:00:00.000000Z INFO: Hello, Bob!
2025-01-13T12:00:00.000000Z INFO: Hello, Charlie!
2025-01-13T12:00:00.000000Z INFO: Hello, World!

````
    </StepCode>
  </Step>

  <Step title="Node.js considerations">
    <StepText>
      **WebSocket support:** Node.js 22+ has native WebSocket support. For Node.js 18-21, the SDK automatically uses the `undici` package (included in devDependencies).

      **Environment variables:** Configure the connection using `SPACETIMEDB_HOST` and `SPACETIMEDB_DB_NAME` environment variables.

      **Exiting:** Press Ctrl+C to stop the client.
    </StepText>
    <StepCode>
```bash
# Configure via environment variables
SPACETIMEDB_HOST=ws://localhost:3000 \
SPACETIMEDB_DB_NAME=my-app \
npm run start

# Or use a .env file with dotenv
````

    </StepCode>

  </Step>
</StepByStep>

## Next steps

- See the [Chat App Tutorial](../00300-tutorials/00100-chat-app.md) for a complete example
- Read the [TypeScript SDK Reference](../../00200-core-concepts/00600-clients/00700-typescript-reference.md) for detailed API docs
