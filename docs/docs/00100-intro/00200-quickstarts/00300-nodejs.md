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

      This will start the local SpacetimeDB server, publish your module, and generate TypeScript bindings.
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

  <Step title="Run the client">
    <StepText>
      In a new terminal, run the Node.js client. It will connect to SpacetimeDB and start an interactive CLI where you can add people and query the database.
    </StepText>
    <StepCode>
```bash
# Run with auto-reload during development
npm run dev

# Or run once
npm run start
```
    </StepCode>
  </Step>

  <Step title="Use the interactive CLI">
    <StepText>
      The client provides a command-line interface to interact with your SpacetimeDB module. Type a name to add a person, or use the built-in commands.
    </StepText>
    <StepCode>
```
Connecting to SpacetimeDB...
  URI: ws://localhost:3000
  Module: nodejs-ts

Connected to SpacetimeDB!
Identity: abc123def456...

Current people (0):
  (none yet)

Commands:
  <name>  - Add a person with that name
  list    - Show all people
  hello   - Greet everyone (check server logs)
  Ctrl+C  - Quit

> Alice
[Added] Alice

> Bob
[Added] Bob

> list
People in database:
  - Alice
  - Bob

> hello
Called say_hello reducer (check server logs)
```
    </StepCode>
  </Step>

  <Step title="Understand the client code">
    <StepText>
      Open `src/main.ts` to see the Node.js client. It uses `DbConnection.builder()` to connect to SpacetimeDB, subscribes to tables, and sets up the interactive CLI using Node's `readline` module.

      Unlike browser apps, Node.js stores the authentication token in a file instead of localStorage.
    </StepText>
    <StepCode>
```typescript
import { DbConnection } from './module_bindings/index.js';

// Build and establish connection
const conn = DbConnection.builder()
  .withUri(HOST)
  .withModuleName(DB_NAME)
  .withToken(loadToken())  // Load saved token from file
  .onConnect((conn, identity, token) => {
    console.log('Connected! Identity:', identity.toHexString());
    saveToken(token);  // Save token for future connections

    // Subscribe to all tables
    conn.subscriptionBuilder()
      .onApplied((ctx) => {
        // Show current data, start CLI
        setupCLI();
      })
      .subscribeToAllTables();

    // Listen for table changes
    conn.db.person.onInsert((ctx, person) => {
      console.log(`[Added] ${person.name}`);
    });
  })
  .build();
```
    </StepCode>
  </Step>

  <Step title="Test with the SpacetimeDB CLI">
    <StepText>
      You can also use the SpacetimeDB CLI to call reducers and query your data directly. Changes made via the CLI will appear in your Node.js client in real-time.
    </StepText>
    <StepCode>
```bash
# Call the add reducer to insert a person
spacetime call <database-name> add Charlie

# Query the person table
spacetime sql <database-name> "SELECT * FROM person"
 name
---------
 "Alice"
 "Bob"
 "Charlie"

# Call say_hello to greet everyone
spacetime call <database-name> say_hello

# View the module logs
spacetime logs <database-name>
2025-01-13T12:00:00.000000Z  INFO: Hello, Alice!
2025-01-13T12:00:00.000000Z  INFO: Hello, Bob!
2025-01-13T12:00:00.000000Z  INFO: Hello, Charlie!
2025-01-13T12:00:00.000000Z  INFO: Hello, World!
```
    </StepCode>
  </Step>

  <Step title="Node.js considerations">
    <StepText>
      **WebSocket support:** Node.js 22+ has native WebSocket support. For Node.js 18-21, the SDK automatically uses the `undici` package (included in devDependencies).

      **Environment variables:** Configure the connection using `SPACETIMEDB_HOST` and `SPACETIMEDB_DB_NAME` environment variables.

      **Graceful shutdown:** The template includes signal handlers for `SIGINT` and `SIGTERM` to cleanly disconnect when stopping the process.
    </StepText>
    <StepCode>
```bash
# Configure via environment variables
SPACETIMEDB_HOST=ws://localhost:3000 \
SPACETIMEDB_DB_NAME=my-app \
npm run start

# Or use a .env file with dotenv
```
    </StepCode>
  </Step>
</StepByStep>

## Next steps

- See the [Chat App Tutorial](/tutorials/chat-app) for a complete example
- Read the [TypeScript SDK Reference](/sdks/typescript) for detailed API docs
