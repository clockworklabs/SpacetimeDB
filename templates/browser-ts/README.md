Get a SpacetimeDB app running in the browser with inline JavaScript.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

Install the [SpacetimeDB CLI](https://spacetimedb.com/install) before continuing.

---

## Create your project

Run the `spacetime dev` command to create a new project with a TypeScript SpacetimeDB module.

This will start the local SpacetimeDB server, publish your module, and generate TypeScript client bindings.

```bash
spacetime dev --template browser-ts
```



## Build the client bindings

The generated TypeScript bindings need to be bundled into a JavaScript file that can be loaded in the browser via a script tag.

```bash
cd my-spacetime-app
npm install
npm run build
```



## Open in browser

Open `index.html` directly in your browser. The app connects to SpacetimeDB and displays data in real-time.

The JavaScript code runs inline in a script tag, using the bundled `DbConnection` class.

:::tip
The browser IIFE bundle also exposes the generated `tables` query builders, so you can use query-builder subscriptions here too.
:::

```html
<!-- Load the bundled bindings -->
<script src="dist/bindings.iife.js"></script>

<script>
  const HOST = 'ws://localhost:3000';
  const DB_NAME = 'my-spacetime-app';
  const TOKEN_KEY = `${HOST}/${DB_NAME}/auth_token`;

  const conn = DbConnection.builder()
    .withUri(HOST)
    .withDatabaseName(DB_NAME)
    .withToken(localStorage.getItem(TOKEN_KEY))
    .onConnect((conn, identity, token) => {
      localStorage.setItem(TOKEN_KEY, token);
      console.log('Connected:', identity.toHexString());

      // Subscribe to tables
      conn.subscriptionBuilder()
        .onApplied(() => {
          for (const person of conn.db.person.iter()) {
            console.log(person.name);
          }
        })
        .subscribe(tables.person);
    })
    .build();
</script>
```



## Call reducers

Reducers are functions that modify data — they're the only way to write to the database.

```javascript
// Call a reducer with named arguments
conn.reducers.add({ name: 'Alice' });
```



## React to changes

Register callbacks to update your UI when data changes.

```javascript
conn.db.person.onInsert((ctx, person) => {
  console.log('New person:', person.name);
});

conn.db.person.onDelete((ctx, person) => {
  console.log('Removed:', person.name);
});
```

## Next steps

- See the [Chat App Tutorial](https://spacetimedb.com/docs/intro/tutorials/chat-app) for a complete example
- Read the [TypeScript SDK Reference](https://spacetimedb.com/docs/intro/core-concepts/clients/typescript-reference) for detailed API docs
