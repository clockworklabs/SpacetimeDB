---
title: Script Tag Quickstart
sidebar_label: Script Tag
slug: /quickstarts/browser
hide_table_of_contents: true
pagination_next: intro/quickstarts/typescript
---

import { InstallCardLink } from "@site/src/components/InstallCardLink";
import { StepByStep, Step, StepText, StepCode } from "@site/src/components/Steps";

Get a SpacetimeDB app running with script tags in under 5 minutes with no build tools required.

## Prerequisites

- [SpacetimeDB CLI](https://spacetimedb.com/install) installed

<InstallCardLink />

---

<StepByStep>
  <Step title="Add the script tag">
    <StepText>
      Create an `index.html` file and load the SpacetimeDB bundle from unpkg. This exposes a global `SpacetimeDB` namespace.
    </StepText>
    <StepCode>
```html
<!doctype html>
<script src="https://unpkg.com/spacetimedb@latest/dist/browser.bundle.js">  
</script>
<script>
  // SpacetimeDB is now available globally
</script>
```
    </StepCode>
  </Step>

  <Step title="Connect to SpacetimeDB">
    <StepText>
      Every connection receives a unique identity from the server.
    </StepText>
    <StepCode>
```javascript
const client = SpacetimeDB.Client.builder()
  .withUri('ws://localhost:3000')
  .withModuleName('my-database')
  .withToken(localStorage.getItem('auth_token') || undefined)
  .onConnect((identityHex, token) => {
    localStorage.setItem('auth_token', token);
    console.log('Connected to SpacetimeDB with identity:', identityHex);
  })
  .onDisconnect(() => {
    console.log('Disconnected from SpacetimeDB');
  })
  .onError(error => {
    console.log('Error connecting to SpacetimeDB:', error);
  })
  .build();

client.connect();
```
    </StepCode>
  </Step>

  <Step title="Subscribe to tables">
    <StepText>
      Tables store your data. When you subscribe to a query, SpacetimeDB sends the matching rows immediately and pushes updates whenever they change.
    </StepText>
    <StepCode>
```javascript
client.subscribe('SELECT * FROM person', (rows) => {
  console.log('People:', rows);
  // [{ name: "Alice" }, { name: "Bob" }]
});
```
    </StepCode>
  </Step>

  <Step title="Call reducers">
    <StepText>
      Reducers are functions that modify data — they're the only way to write to the database.
    </StepText>
    <StepCode>
```javascript
await client.call('add', { name: 'Alice' });
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
spacetime call <database-name> add Alice

# Query the person table
spacetime sql <database-name> "SELECT * FROM person"
 name
---------
 "Alice"

# Call say_hello to greet everyone
spacetime call <database-name> say_hello

# View the module logs
spacetime logs <database-name>
2025-01-13T12:00:00.000000Z  INFO: Hello, Alice!
2025-01-13T12:00:00.000000Z  INFO: Hello, World!
```
    </StepCode>
  </Step>
</StepByStep>

## Next steps

- Use `spacetime dev --template browser-ts` for a full project template with script tags
- See the [Chat App Tutorial](/tutorials/chat-app) for a complete example
- Read the [TypeScript SDK Reference](/sdks/typescript) for detailed API docs
