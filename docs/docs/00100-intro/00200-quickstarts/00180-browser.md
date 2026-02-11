---
title: Browser Quickstart
sidebar_label: Browser
slug: /quickstarts/browser
hide_table_of_contents: true
pagination_next: intro/quickstarts/typescript
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { StepByStep, Step, StepText, StepCode } from "@site/src/components/Steps";

Get a SpacetimeDB app running in the browser with inline JavaScript.

## Prerequisites

- [Node.js](https://nodejs.org/) 18+ installed
- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

---

<StepByStep>
  <Step title="Create your project">
    <StepText>
      Run the `spacetime dev` command to create a new project with a TypeScript SpacetimeDB module.

      This will start the local SpacetimeDB server, publish your module, and generate TypeScript client bindings.
    </StepText>
    <StepCode>
```bash
spacetime dev --template browser-ts my-spacetime-app
```
    </StepCode>
  </Step>

  <Step title="Build the client bindings">
    <StepText>
      The generated TypeScript bindings need to be bundled into a JavaScript file that can be loaded in the browser via a script tag.
    </StepText>
    <StepCode>
```bash
cd my-spacetime-app
npm install
npm run build
```
    </StepCode>
  </Step>

  <Step title="Open in browser">
    <StepText>
      Open `index.html` directly in your browser. The app connects to SpacetimeDB and displays data in real-time.

      The JavaScript code runs inline in a script tag, using the bundled `DbConnection` class.
    </StepText>
    <StepCode>
```html
<!-- Load the bundled bindings -->
<script src="dist/bindings.iife.js"></script>

<script>
  const conn = DbConnection.builder()
    .withUri('ws://localhost:3000')
    .withDatabaseName('my-spacetime-app')
    .withToken(localStorage.getItem('auth_token'))
    .onConnect((conn, identity, token) => {
      localStorage.setItem('auth_token', token);
      console.log('Connected:', identity.toHexString());

      // Subscribe to tables
      conn.subscriptionBuilder()
        .onApplied(() => {
          for (const person of conn.db.person.iter()) {
            console.log(person.name);
          }
        })
        .subscribe(['SELECT * FROM person']);
    })
    .build();
</script>
```
    </StepCode>
  </Step>

  <Step title="Call reducers">
    <StepText>
      Reducers are functions that modify data — they're the only way to write to the database.
    </StepText>
    <StepCode>
```javascript
// Call a reducer with named arguments
conn.reducers.add({ name: 'Alice' });
```
    </StepCode>
  </Step>

  <Step title="React to changes">
    <StepText>
      Register callbacks to update your UI when data changes.
    </StepText>
    <StepCode>
```javascript
conn.db.person.onInsert((ctx, person) => {
  console.log('New person:', person.name);
});

conn.db.person.onDelete((ctx, person) => {
  console.log('Removed:', person.name);
});
```
    </StepCode>
  </Step>
</StepByStep>

## Next steps

- See the [Chat App Tutorial](/tutorials/chat-app) for a complete example
- Read the [TypeScript SDK Reference](/sdks/typescript) for detailed API docs
