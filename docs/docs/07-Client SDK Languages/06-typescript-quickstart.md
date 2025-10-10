---
title: TypeScript Quickstart
slug: /sdks/typescript/quickstart
---

# TypeScript Client SDK Quickstart

In this guide, you'll learn how to use TypeScript to create a SpacetimeDB client application.

Please note that TypeScript is supported as a client language only. **Before you get started on this guide**, you should complete one of the quickstart guides for creating a SpacetimeDB server module listed below.

- [Rust](/modules/rust/quickstart)
- [C#](/modules/c-sharp/quickstart)

By the end of this introduction, you will have created a basic single page web app which connects to the `quickstart-chat` database created in the above module quickstart guides.

## Project structure

Enter the directory `quickstart-chat` you created in the [Rust Module Quickstart](/modules/rust/quickstart) or [C# Module Quickstart](/modules/c-sharp/quickstart) guides:

```bash
cd quickstart-chat
```

Within it, create a `client` React app:

```bash
pnpm create vite@latest client -- --template react-ts
cd client
pnpm install
```

We also need to install the `spacetime-client-sdk` package:

```bash
pnpm install spacetimedb
```

:::note

If you are using another package manager like `yarn` or `npm`, the same steps should work with the appropriate commands for those tools.

:::

:::warning

The `@clockworklabs/spacetimedb-sdk` package has been deprecated in favor of the `spacetimedb` package as of SpacetimeDB version 1.4.0. If you are using the old SDK package, you will need to switch to `spacetimedb`. You will also need a SpacetimeDB CLI version of 1.4.0+ to generate bindings for the new `spacetimedb` package.

:::

You can now `pnpm run dev` to see the Vite template app running at `http://localhost:5173`.

## Basic layout

The app we're going to create is a basic chat application. We will begin by creating a layout for our app. The webpage will contain four sections:

1. A profile section, where we can set our name.
2. A message section, where we can see all the messages.
3. A system section, where we can see system messages.
4. A new message section, where we can send a new message.

Replace the entire contents of `client/src/App.tsx` with the following:

```tsx
import React, { useEffect, useState } from 'react';
import { DbConnection, Message, User } from './module_bindings';
import { useSpacetimeDB, useTable, where, eq } from 'spacetimedb/react';
import { Identity, Timestamp } from 'spacetimedb';
import './App.css';

export type PrettyMessage = {
  senderName: string;
  text: string;
  sent: Timestamp;
  kind: 'system' | 'user';
};

function App() {
  const [newName, setNewName] = useState('');
  const [settingName, setSettingName] = useState(false);
  const [systemMessages, setSystemMessages] = useState([] as Message[]);
  const [newMessage, setNewMessage] = useState('');

  const prettyMessages: PrettyMessage[] = [];
  const onlineUsers: User[] = [];
  const offlineUsers: User[] = [];
  const users = [...onlineUsers, ...offlineUsers];

  const name = '';

  const onSubmitNewName = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setSettingName(false);
    // TODO: Call `setName` reducer
  };

  const onSubmitMessage = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    setNewMessage('');
    // TODO: Call `sendMessage` reducer
  };

  return (
    <div className="App">
      <div className="profile">
        <h1>Profile</h1>
        {!settingName ? (
          <>
            <p>{name}</p>
            <button
              onClick={() => {
                setSettingName(true);
                setNewName(name);
              }}
            >
              Edit Name
            </button>
          </>
        ) : (
          <form onSubmit={onSubmitNewName}>
            <input
              type="text"
              aria-label="username input"
              value={newName}
              onChange={e => setNewName(e.target.value)}
            />
            <button type="submit">Submit</button>
          </form>
        )}
      </div>
      <div className="message-panel">
        <h1>Messages</h1>
        {prettyMessages.length < 1 && <p>No messages</p>}
        <div className="messages">
          {prettyMessages.map((message, key) => {
            const sentDate = message.sent.toDate();
            const now = new Date();
            const isOlderThanDay =
              now.getFullYear() !== sentDate.getFullYear() ||
              now.getMonth() !== sentDate.getMonth() ||
              now.getDate() !== sentDate.getDate();

            const timeString = sentDate.toLocaleTimeString([], {
              hour: '2-digit',
              minute: '2-digit',
            });
            const dateString = isOlderThanDay
              ? sentDate.toLocaleDateString([], {
                  year: 'numeric',
                  month: 'short',
                  day: 'numeric',
                }) + ' '
              : '';

            return (
              <div
                key={key}
                className={
                  message.kind === 'system' ? 'system-message' : 'user-message'
                }
              >
                <p>
                  <b>
                    {message.kind === 'system' ? 'System' : message.senderName}
                  </b>
                  <span
                    style={{
                      fontSize: '0.8rem',
                      marginLeft: '0.5rem',
                      color: '#666',
                    }}
                  >
                    {dateString}
                    {timeString}
                  </span>
                </p>
                <p>{message.text}</p>
              </div>
            );
          })}
        </div>
      </div>
      <div className="online" style={{ whiteSpace: 'pre-wrap' }}>
        <h1>Online</h1>
        <div>
          {onlineUsers.map((user, key) => (
            <div key={key}>
              <p>{user.name || user.identity.toHexString().substring(0, 8)}</p>
            </div>
          ))}
        </div>
        {offlineUsers.length > 0 && (
          <div>
            <h1>Offline</h1>
            {offlineUsers.map((user, key) => (
              <div key={key}>
                <p>
                  {user.name || user.identity.toHexString().substring(0, 8)}
                </p>
              </div>
            ))}
          </div>
        )}
      </div>
      <div className="new-message">
        <form
          onSubmit={onSubmitMessage}
          style={{
            display: 'flex',
            flexDirection: 'column',
            width: '50%',
            margin: '0 auto',
          }}
        >
          <h3>New Message</h3>
          <textarea
            aria-label="message input"
            value={newMessage}
            onChange={e => setNewMessage(e.target.value)}
          ></textarea>
          <button type="submit">Send</button>
        </form>
      </div>
    </div>
  );
}

export default App;
```

We have configured the `onSubmitNewName` and `onSubmitMessage` callbacks to be called when the user clicks the submit button in the profile and new message sections, respectively. For now, they do nothing when called, but later we'll add some logic to call SpacetimeDB reducers when these callbacks are called.

Let's also make it pretty. Replace the contents of `client/src/App.css` with the following:

```css
.App {
  display: grid;
  /* 
    3 rows: 
      1) Profile
      2) Main content (left = message, right = online)
      3) New message
  */
  grid-template-rows: auto 1fr auto;
  /* 2 columns: left for chat, right for online */
  grid-template-columns: 2fr 1fr;

  height: 100vh; /* fill viewport height */
  width: clamp(300px, 100%, 1200px);
  margin: 0 auto;
}

/* ----- Profile (Row 1, spans both columns) ----- */
.profile {
  grid-column: 1 / 3;
  display: flex;
  align-items: center;
  gap: 1rem;
  padding: 1rem;
  border-bottom: 1px solid var(--theme-color);
}

.profile h1 {
  margin-right: auto; /* pushes name/edit form to the right */
}

.profile form {
  display: flex;
  flex-grow: 1;
  align-items: center;
  gap: 0.5rem;
  max-width: 300px;
}

.profile form input {
  background-color: var(--textbox-color);
}

/* ----- Chat Messages (Row 2, Col 1) ----- */
.message-panel {
  grid-row: 2 / 3;
  grid-column: 1 / 2;

  /* Ensure this section scrolls if content is long */
  overflow-y: auto;
  padding: 1rem;
  display: flex;
  flex-direction: column;
  gap: 1rem;
}

.messages {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.system-message {
  background-color: var(--theme-color);
  color: var(--theme-color-contrast);
  padding: 0.5rem 1rem;
  border-radius: 0.375rem;
  font-style: italic;
}

.user-message {
  background-color: var(--textbox-color);
  padding: 0.5rem 1rem;
  border-radius: 0.375rem;
}

.message h1 {
  margin-right: 0.5rem;
}

/* ----- Online Panel (Row 2, Col 2) ----- */
.online {
  grid-row: 2 / 3;
  grid-column: 2 / 3;

  /* Also scroll independently if needed */
  overflow-y: auto;
  padding: 1rem;
  border-left: 1px solid var(--theme-color);
  white-space: pre-wrap;
  font-family: monospace;
}

/* ----- New Message (Row 3, spans columns 1-2) ----- */
.new-message {
  grid-column: 1 / 3;
  display: flex;
  justify-content: center;
  align-items: center;
  padding: 1rem;
  border-top: 1px solid var(--theme-color);
}

.new-message form {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  width: 100%;
  max-width: 600px;
}

.new-message form h3 {
  margin-bottom: 0.25rem;
}

/* Distinct background for the textarea */
.new-message form textarea {
  font-family: monospace;
  font-weight: 400;
  font-size: 1rem;
  resize: vertical;
  min-height: 80px;
  background-color: var(--textbox-color);
  color: inherit;

  /* Subtle shadow for visibility */
  box-shadow:
    0 1px 3px rgba(0, 0, 0, 0.12),
    0 1px 2px rgba(0, 0, 0, 0.24);
}

@media (prefers-color-scheme: dark) {
  .new-message form textarea {
    box-shadow: 0 0 0 1px #17492b;
  }
}
```

Next, we need to replace the global styles in `client/src/index.css` as well:

```css
/* ----- CSS Reset & Global Settings ----- */
*,
*::before,
*::after {
  box-sizing: border-box;
  margin: 0;
  padding: 0;
}

/* ----- Color Variables ----- */
:root {
  --theme-color: #3dc373;
  --theme-color-contrast: #08180e;
  --textbox-color: #edfef4;
  color-scheme: light dark;
}

@media (prefers-color-scheme: dark) {
  :root {
    --theme-color: #4cf490;
    --theme-color-contrast: #132219;
    --textbox-color: #0f311d;
  }
}

/* ----- Page Setup ----- */
html,
body,
#root {
  height: 100%;
  margin: 0;
}

body {
  font-family:
    -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', 'Oxygen', 'Ubuntu',
    'Cantarell', 'Fira Sans', 'Droid Sans', 'Helvetica Neue', sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

code {
  font-family:
    source-code-pro, Menlo, Monaco, Consolas, 'Courier New', monospace;
}

/* ----- Buttons ----- */
button {
  padding: 0.5rem 0.75rem;
  border: none;
  border-radius: 0.375rem;
  background-color: var(--theme-color);
  color: var(--theme-color-contrast);
  cursor: pointer;
  font-weight: 600;
  letter-spacing: 0.1px;
  font-family: monospace;
}

/* ----- Inputs & Textareas ----- */
input,
textarea {
  border: none;
  border-radius: 0.375rem;
  caret-color: var(--theme-color);
  font-family: monospace;
  font-weight: 600;
  letter-spacing: 0.1px;
  padding: 0.5rem 0.75rem;
}

input:focus,
textarea:focus {
  outline: none;
  box-shadow: 0 0 0 2px var(--theme-color);
}
```

Now, when you run `pnpm run dev` and open `http://localhost:5173`, you should see a basic chat app that does not yet send or receive messages.

## Generate your module types

The `spacetime` CLI's `generate` command generates client-side interfaces for the tables, reducers, and types defined in your server module.

In your `quickstart-chat` directory, run:

```bash
mkdir -p client/src/module_bindings
spacetime generate --lang typescript --out-dir client/src/module_bindings --project-path server
```

:::note

This command assumes you've already created a server module in `quickstart-chat/server`. If you haven't completed one of the server module quickstart guides, you can follow either the [Rust](/modules/rust/quickstart) or [C#](/modules/c-sharp/quickstart) module quickstart to create one and then return here.

:::

Take a look inside `client/src/module_bindings`. The CLI should have generated several files:

```
module_bindings
├── identity_connected_reducer.ts
├── identity_disconnected_reducer.ts
├── index.ts
├── init_reducer.ts
├── message_table.ts
├── message_type.ts
├── send_message_reducer.ts
├── set_name_reducer.ts
├── user_table.ts
└── user_type.ts
```

With `spacetime generate` we have generated TypeScript types derived from the types you specified in your module, which we can conveniently use in our client. We've placed these in the `module_bindings` folder.
Now that we've set up our UI and generated our types, let's connect to SpacetimeDB.

The main entry to the SpacetimeDB API is the `DbConnection`, a type that manages a connection to a remote database. Let's import it and a few other types into our `client/src/main.tsx` below our other imports:

```tsx
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import './index.css';
import App from './App.tsx';
import { Identity } from 'spacetimedb';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection, ErrorContext } from './module_bindings/index.ts';
```

Note that we are importing `DbConnection` from our `module_bindings` so that it has all the type information about our tables and types.

We've also imported the `SpacetimeDBProvider` React component which will allow us to connect our SpacetimeDB state directly to our React state seamlessly.

## Create your SpacetimeDB client

Now that we've imported the `DbConnection` type, we can use it to connect our app to our database.

Replace the body of the `main.tsx` file with the following, just below your imports:

```tsx
const onConnect = (conn: DbConnection, identity: Identity, token: string) => {
  localStorage.setItem('auth_token', token);
  console.log(
    'Connected to SpacetimeDB with identity:',
    identity.toHexString()
  );
  conn.reducers.onSendMessage(() => {
    console.log('Message sent.');
  });
};

const onDisconnect = () => {
  console.log('Disconnected from SpacetimeDB');
};

const onConnectError = (_ctx: ErrorContext, err: Error) => {
  console.log('Error connecting to SpacetimeDB:', err);
};

const connectionBuilder = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withModuleName('quickstart-chat')
  .withToken(localStorage.getItem('auth_token') || undefined)
  .onConnect(onConnect)
  .onDisconnect(onDisconnect)
  .onConnectError(onConnectError);

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <App />
    </SpacetimeDBProvider>
  </StrictMode>
);
```

Here we are configuring our SpacetimeDB connection by specifying the server URI, database name, and a few callbacks including the `onConnect` callback. When `onConnect` is called after connecting, we store the connection state, our `Identity`, and our SpacetimeDB credentials in our React state. If there is an error connecting, we also print that error to the console.

We are also using `localStorage` to store our SpacetimeDB credentials. This way, we can reconnect to SpacetimeDB with the same `Identity` and token if we refresh the page. The first time we connect, we won't have any credentials stored, so we pass `undefined` to the `withToken` method. This will cause SpacetimeDB to generate new credentials for us.

If you chose a different name for your database, replace `quickstart-chat` with that name, or republish your module as `quickstart-chat`.

Our React hooks will subscribe to the data in SpacetimeDB. When we subscribe, SpacetimeDB will run our subscription queries and store the result in a local "client cache". This cache will be updated in real-time as the data in the table changes on the server.

We pass our connection configuration directly to the `SpacetimeDBProvider`, which will manage our connection to SpacetimeDB.

### Accessing the Data

Once SpacetimeDB is connected, we can easily access the data in the client cache using SpacetimeDB's provided React hooks, `useTable` and `useSpacetimeDB`.

`useTable` is the simplest way to access your database data. `useTable` subscribes your React app to data in a SpacetimeDB table so that it updates as the data changes. It essentially acts just like `useState` in React except the data is being updated in real-time from SpacetimeDB tables.

`useSpacetimeDB` gives you direct access to the connection in case you want to check the state of the connection or access database table state. Note that `useSpacetimeDB` does not automatically subscribe your app to data in the database.

Add the following `useSpacetimeDB` hook to the top of your render function, just below your `useState` declarations.

```tsx
const conn = useSpacetimeDB<DbConnection>();
const { identity, isActive: connected } = conn;

// Subscribe to all messages in the chat
const { rows: messages } = useTable<DbConnection, Message>('message');
```

Next replace `const onlineUsers: User[] = [];` with the following:

```tsx
// Subscribe to all online users in the chat
// so we can show who's online and demonstrate
// the `where` and `eq` query expressions
const { rows: onlineUsers } = useTable<DbConnection, User>(
  'user',
  where(eq('online', true))
);
```

Notice that we can filter users in the `user` table based on their online status by passing a query expression into the `useTable` hook as the second argument.

Let's now prettify our messages in our render function by sorting them by their `sent` timestamp, and joining the username of the sender to the message by looking up the user by their `Identity` in the `user` table. Replace `const prettyMessages: PrettyMessage[] = [];` with the following:

```tsx
const prettyMessages: PrettyMessage[] = Array.from(messages)
  .sort((a, b) => (a.sent.toDate() > b.sent.toDate() ? 1 : -1))
  .map(message => {
    const user = users.find(
      u => u.identity.toHexString() === message.sender.toHexString()
    );
    return {
      senderName: user?.name || message.sender.toHexString().substring(0, 8),
      text: message.text,
      sent: message.sent,
      kind: Identity.zero().isEqual(message.sender) ? 'system' : 'user',
    };
  });
```

That's all we have to do to hook up our SpacetimeDB state to our React state. SpacetimeDB ensures that any changes on the server are pushed down to our application and rerendered on screen in real-time.

Let's also update our render function to show a loading message while we're connecting to SpacetimeDB. Add this just below our `prettyMessages` declaration:

```tsx
if (!connected || !identity) {
  return (
    <div className="App">
      <h1>Connecting...</h1>
    </div>
  );
}
```

Finally, let's also compute the name of the user from the `Identity` in our `name` variable. Replace `const name = '';` with the following:

```tsx
const name = (() => {
  const user = users.find(u => u.identity.isEqual(identity));
  return user?.name || identity?.toHexString().substring(0, 8) || '';
})();
```

### Calling Reducers

Let's hook up our callbacks so we can send some messages and see them displayed in the app after they are synchronised by SpacetimeDB. We need to update the `onSubmitNewName` and `onSubmitMessage` callbacks to send the appropriate reducer to the module.

Modify the `onSubmitNewName` callback by adding a call to the `setName` reducer:

```tsx
const onSubmitNewName = (e: React.FormEvent<HTMLFormElement>) => {
  e.preventDefault();
  setSettingName(false);
  conn.reducers.setName(newName);
};
```

Next, modify the `onSubmitMessage` callback by adding a call to the `sendMessage` reducer:

```tsx
const onSubmitMessage = (e: React.FormEvent<HTMLFormElement>) => {
  e.preventDefault();
  setNewMessage('');
  conn.reducers.sendMessage(newMessage);
};
```

SpacetimeDB generated these functions for us based on the type information provided by our module. Calling these functions will invoke our reducers in our module.

Let's try out our app to see the result of these changes.

```sh
cd client
pnpm run dev
```

:::warning

Don't forget! You may need to publish your server module if you haven't yet.

:::

Send some messages and update your username and watch it change in real-time. Note that when you update your username, it also updates immediately for all prior messages. This is because the messages store the user's `Identity` directly, instead of their username, so we can retroactively apply their username to all prior messages.

Try opening a few incognito windows to see what it's like with multiple users!

### Notify about new users

We can also register `onInsert`, `onUpdate`, and `onDelete` callbacks to handle events, not just state. For example, we might want to show a notification any time a new user connects to the database.

Note that these callbacks can fire in two contexts:

- After a reducer runs, when the client's cache is updated about changes to subscribed rows.
- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.

Our current `useTable` only filters online users, but we can print a system message anytime a user enters or leaves the room by subscribing to callbacks on the `onlineUsers` React hook.

Update your `onlineUsers` React hook to add the following callbacks:

```tsx
// Subscribe to all online users in the chat
// so we can show who's online and demonstrate
// the `where` and `eq` query expressions
const { rows: onlineUsers } = useTable<DbConnection, User>(
  'user',
  where(eq('online', true)),
  {
    onInsert: user => {
      // All users being inserted here are online
      const name = user.name || user.identity.toHexString().substring(0, 8);
      setSystemMessages(prev => [
        ...prev,
        {
          sender: Identity.zero(),
          text: `${name} has connected.`,
          sent: Timestamp.now(),
        },
      ]);
    },
    onDelete: user => {
      // All users being deleted here are offline
      const name = user.name || user.identity.toHexString().substring(0, 8);
      setSystemMessages(prev => [
        ...prev,
        {
          sender: Identity.zero(),
          text: `${name} has disconnected.`,
          sent: Timestamp.now(),
        },
      ]);
    },
  }
);
```

These callbacks will be called any time the state of the `useTable` result changes to add or remove a row, while respecting your `where` filter.

Here, we post a system message indicating that a new user has connected if the user is being added to the `user` table and they're online, or if an existing user's online status is being updated to "online".

Next, let's add the system messages to our list of `Message`s so they can be interleaved with the chat messages. Modify `prettyMessages` to concat the `systemMessages` as well:

```tsx
const prettyMessages: PrettyMessage[] = Array.from(messages)
  .concat(systemMessages)
  .sort((a, b) => (a.sent.toDate() > b.sent.toDate() ? 1 : -1))
  .map(message => {
    const user = users.find(
      u => u.identity.toHexString() === message.sender.toHexString()
    );
    return {
      senderName: user?.name || message.sender.toHexString().substring(0, 8),
      text: message.text,
      sent: message.sent,
      kind: Identity.zero().isEqual(message.sender) ? 'system' : 'user',
    };
  });
```

Finally, let's also subscribe to offline users so we can show them in the sidebar as well. Replace `const offlineUsers: User[] = [];` with:

```tsx
const { rows: offlineUsers } = useTable<DbConnection, User>(
  'user',
  where(eq('online', false))
);
```

## Conclusion

Congratulations! You've built a simple chat app with SpacetimeDB. You can find the full source code for the client we've created in this quickstart tutorial [here](https://github.com/clockworklabs/SpacetimeDB/tree/master/crates/bindings-typescript/examples/quickstart-chat).

At this point you've learned how to create a basic TypeScript client for your SpacetimeDB `quickstart-chat` module. You've learned how to connect to SpacetimeDB and call reducers to update data. You've learned how to subscribe to table data, and hook it up so that it updates reactively in a React application.

## What's next?

We covered a lot here, but we haven't covered everything. Take a look at our [reference documentation](/sdks/typescript) to find out how you can use SpacetimeDB in more advanced ways, including managing reducer errors and subscribing to reducer events.
