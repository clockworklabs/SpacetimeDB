---
title: Typescript Client SDK Quick Start
navTitle: Typescript Quickstart
---

In this guide we'll show you how to get up and running with a simple SpacetimDB app with a client written in Typescript.

We'll implement a basic single page web app for the module created in our Rust or C# Module Quickstart guides. **Make sure you follow one of these guides before you start on this one.**

## Project structure

Enter the directory `quickstart-chat` you created in the [Rust Module Quickstart](/docs/modules/rust/quickstart) or [C# Module Quickstart](/docs/modules/c-sharp/quickstart) guides:

```bash
cd quickstart-chat
```

Within it, create a `client` react app:

```bash
npx create-react-app client --template typescript
```

We also need to install the `spacetime-client-sdk` package:

```bash
cd client
npm install @clockworklabs/spacetimedb-sdk
```

## Basic layout

We are going to start by creating a basic layout for our app. The page contains four sections:

1. A profile section, where we can set our name.
2. A message section, where we can see all the messages.
3. A system section, where we can see system messages.
4. A new message section, where we can send a new message.

The `onSubmitNewName` and `onMessageSubmit` callbacks will be called when the user clicks the submit button in the profile and new message sections, respectively. We'll hook these up later.

Replace the entire contents of `client/src/App.tsx` with the following:

```typescript
import React, { useEffect, useState } from 'react';
import logo from './logo.svg';
import './App.css';

export type MessageType = {
	name: string;
	message: string;
};

function App() {
	const [newName, setNewName] = useState('');
	const [settingName, setSettingName] = useState(false);
	const [name, setName] = useState('');
	const [systemMessage, setSystemMessage] = useState('');
	const [messages, setMessages] = useState<MessageType[]>([]);

	const [newMessage, setNewMessage] = useState('');

	const onSubmitNewName = (e: React.FormEvent<HTMLFormElement>) => {
		e.preventDefault();
		setSettingName(false);
		// Fill in app logic here
	};

	const onMessageSubmit = (e: React.FormEvent<HTMLFormElement>) => {
		e.preventDefault();
		// Fill in app logic here
		setNewMessage('');
	};

	return (
		<div className='App'>
			<div className='profile'>
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
							type='text'
							style={{ marginBottom: '1rem' }}
							value={newName}
							onChange={(e) => setNewName(e.target.value)}
						/>
						<button type='submit'>Submit</button>
					</form>
				)}
			</div>
			<div className='message'>
				<h1>Messages</h1>
				{messages.length < 1 && <p>No messages</p>}
				<div>
					{messages.map((message, key) => (
						<div key={key}>
							<p>
								<b>{message.name}</b>
							</p>
							<p>{message.message}</p>
						</div>
					))}
				</div>
			</div>
			<div className='system' style={{ whiteSpace: 'pre-wrap' }}>
				<h1>System</h1>
				<div>
					<p>{systemMessage}</p>
				</div>
			</div>
			<div className='new-message'>
				<form
					onSubmit={onMessageSubmit}
					style={{
						display: 'flex',
						flexDirection: 'column',
						width: '50%',
						margin: '0 auto',
					}}
				>
					<h3>New Message</h3>
					<textarea value={newMessage} onChange={(e) => setNewMessage(e.target.value)}></textarea>
					<button type='submit'>Send</button>
				</form>
			</div>
		</div>
	);
}

export default App;
```

Now when you run `npm start`, you should see a basic chat app that does not yet send or receive messages.

## Generate your module types

The `spacetime` CLI's `generate` command will generate client-side interfaces for the tables, reducers and types defined in your server module.

In your `quickstart-chat` directory, run:

```bash
mkdir -p client/src/module_bindings
spacetime generate --lang typescript --out-dir client/src/module_bindings --project-path server
```

Take a look inside `client/src/module_bindings`. The CLI should have generated four files:

```
module_bindings
├── message.ts
├── send_message_reducer.ts
├── set_name_reducer.ts
└── user.ts
```

We need to import these types into our `client/src/App.tsx`. While we are at it, we will also import the SpacetimeDBClient class from our SDK. In order to let the SDK know what tables and reducers we will be using we need to also register them.

```typescript
import {
  SpacetimeDBClient,
  Identity,
  Address,
} from '@clockworklabs/spacetimedb-sdk';

import Message from './module_bindings/message';
import User from './module_bindings/user';
import SendMessageReducer from './module_bindings/send_message_reducer';
import SetNameReducer from './module_bindings/set_name_reducer';

SpacetimeDBClient.registerReducers(SendMessageReducer, SetNameReducer);
SpacetimeDBClient.registerTables(Message, User);
```

## Create your SpacetimeDB client

First, we need to create a SpacetimeDB client and connect to the module. Create your client at the top of the `App` function.

We are going to create a stateful variable to store our client's SpacetimeDB identity when we receive it. Also, we are using `localStorage` to retrieve your auth token if this client has connected before. We will explain these later.

Replace `<module-name>` with the name you chose when publishing your module during the module quickstart. If you are using SpacetimeDB Cloud, the host will be `wss://spacetimedb.com/spacetimedb`.

Add this before the `App` function declaration:

```typescript
let token = localStorage.getItem('auth_token') || undefined;
var spacetimeDBClient = new SpacetimeDBClient(
  'ws://localhost:3000',
  'chat',
  token
);
```

Inside the `App` function, add a few refs:

```typescript
let local_identity = useRef<Identity | undefined>(undefined);
let initialized = useRef<boolean>(false);
const client = useRef<SpacetimeDBClient>(spacetimeDBClient);
```

## Register callbacks and connect

We need to handle several sorts of events:

1. `onConnect`: When we connect and receive our credentials, we'll save them to browser local storage, so that the next time we connect, we can re-authenticate as the same user.
2. `initialStateSync`: When we're informed of the backlog of past messages, we'll sort them and update the `message` section of the page.
3. `Message.onInsert`: When we receive a new message, we'll update the `message` section of the page.
4. `User.onInsert`: When a new user joins, we'll update the `system` section of the page with an appropiate message.
5. `User.onUpdate`: When a user is updated, we'll add a message with their new name, or declare their new online status to the `system` section of the page.
6. `SetNameReducer.on`: If the server rejects our attempt to set our name, we'll update the `system` section of the page with an appropriate error message.
7. `SendMessageReducer.on`: If the server rejects a message we send, we'll update the `system` section of the page with an appropriate error message.

We will add callbacks for each of these items in the following sections. All of these callbacks will be registered inside the `App` function after the `useRef` declarations.

### onConnect Callback

On connect SpacetimeDB will provide us with our client credentials.

Each user has a set of credentials, which consists of two parts:

- An `Identity`, a unique public identifier. We're using these to identify `User` rows.
- A `Token`, a private key which SpacetimeDB uses to authenticate the client.

These credentials are generated by SpacetimeDB each time a new client connects, and sent to the client so they can be saved, in order to re-connect with the same identity.

We want to store our local client identity in a stateful variable and also save our `token` to local storage for future connections.

Each client also has an `Address`, which modules can use to distinguish multiple concurrent connections by the same `Identity`. We don't need to know our `Address`, so we'll ignore that argument.

Once we are connected, we can send our subscription to the SpacetimeDB module. SpacetimeDB is set up so that each client subscribes via SQL queries to some subset of the database, and is notified about changes only to that subset. For complex apps with large databases, judicious subscriptions can save each client significant network bandwidth, memory and computation compared. For example, in [BitCraft](https://bitcraftonline.com), each player's client subscribes only to the entities in the "chunk" of the world where that player currently resides, rather than the entire game world. Our app is much simpler than BitCraft, so we'll just subscribe to the whole database.

To the body of `App`, add:

```typescript
client.current.onConnect((token, identity, address) => {
  console.log('Connected to SpacetimeDB');

  local_identity.current = identity;

  localStorage.setItem('auth_token', token);

  client.current.subscribe(['SELECT * FROM User', 'SELECT * FROM Message']);
});
```

### initialStateSync callback

This callback fires when our local client cache of the database is populated. This is a good time to set the initial messages list.

We'll define a helper function, `setAllMessagesInOrder`, to supply the `MessageType` class for our React application. It will call the autogenerated `Message.all` function to get an array of `Message` rows, then sort them and convert them to `MessageType`.

To find the `User` based on the message's `sender` identity, we'll use `User::findByIdentity`, which behaves like the same function on the server.

Whenever we want to display a user name, if they have set a name, we'll use that. If they haven't set a name, we'll instead use the first 8 bytes of their identity, encoded as hexadecimal. We'll define the function `userNameOrIdentity` to handle this.

We also have to handle the case where we don't find a matching `User` row. This can happen when the module owner sends a message using the CLI's `spacetime call`. In this case, we'll display `unknown`.

To the body of `App`, add:

```typescript
function userNameOrIdentity(user: User): string {
  console.log(`Name: ${user.name} `);
  if (user.name !== null) {
    return user.name || '';
  } else {
    var identityStr = new Identity(user.identity).toHexString();
    console.log(`Name: ${identityStr} `);
    return new Identity(user.identity).toHexString().substring(0, 8);
  }
}

function setAllMessagesInOrder() {
  let messages = Array.from(Message.all());
  messages.sort((a, b) => (a.sent > b.sent ? 1 : a.sent < b.sent ? -1 : 0));

  let messagesType: MessageType[] = messages.map(message => {
    let sender_identity = User.findByIdentity(message.sender);
    let display_name = sender_identity
      ? userNameOrIdentity(sender_identity)
      : 'unknown';

    return {
      name: display_name,
      message: message.text,
    };
  });

  setMessages(messagesType);
}

client.current.on('initialStateSync', () => {
  setAllMessagesInOrder();
  var user = User.findByIdentity(local_identity?.current?.toUint8Array()!);
  setName(userNameOrIdentity(user!));
});
```

### Message.onInsert callback - Update messages

When we receive a new message, we'll update the messages section of the page. Keep in mind that we only want to do this for new messages, i.e. those inserted by a `send_message` reducer invocation. When the server is initializing our cache, we'll get a callback for each existing message, but we don't want to update the page for those. To that effect, our `onInsert` callback will check if its `ReducerEvent` argument is not `undefined`, and only update the `message` section in that case.

To the body of `App`, add:

```typescript
Message.onInsert((message, reducerEvent) => {
  if (reducerEvent !== undefined) {
    setAllMessagesInOrder();
  }
});
```

### User.onInsert callback - Notify about new users

For each table, we can register on-insert and on-delete callbacks to be run whenever a subscribed row is inserted or deleted. We register these callbacks using the `onInsert` and `onDelete` methods of the trait `TableType`, which is automatically implemented for each table by `spacetime generate`.

These callbacks can fire in two contexts:

- After a reducer runs, when the client's cache is updated about changes to subscribed rows.
- After calling `subscribe`, when the client's cache is initialized with all existing matching rows.

This second case means that, even though the module only ever inserts online users, the client's `User.onInsert` callbacks may be invoked with users who are offline. We'll only notify about online users.

`onInsert` and `onDelete` callbacks take two arguments: the altered row, and a `ReducerEvent | undefined`. This will be `undefined` for rows inserted when initializing the cache for a subscription. `ReducerEvent` is a class containing information about the reducer that triggered this event. For now, we can ignore this argument.

We are going to add a helper function called `appendToSystemMessage` that will append a line to the `systemMessage` state. We will use this to update the `system` message when a new user joins.

To the body of `App`, add:

```typescript
// Helper function to append a line to the systemMessage state
function appendToSystemMessage(line: String) {
  setSystemMessage(prevMessage => prevMessage + '\n' + line);
}

User.onInsert((user, reducerEvent) => {
  if (user.online) {
    appendToSystemMessage(`${userNameOrIdentity(user)} has connected.`);
  }
});
```

### User.onUpdate callback - Notify about updated users

Because we declared a `#[primarykey]` column in our `User` table, we can also register on-update callbacks. These run whenever a row is replaced by a row with the same primary key, like our module's `User::update_by_identity` calls. We register these callbacks using the `onUpdate` method which is automatically implemented by `spacetime generate` for any table with a `#[primarykey]` column.

`onUpdate` callbacks take three arguments: the old row, the new row, and a `ReducerEvent`.

In our module, users can be updated for three reasons:

1. They've set their name using the `set_name` reducer.
2. They're an existing user re-connecting, so their `online` has been set to `true`.
3. They've disconnected, so their `online` has been set to `false`.

We'll update the `system` message in each of these cases.

To the body of `App`, add:

```typescript
User.onUpdate((oldUser, user, reducerEvent) => {
  if (oldUser.online === false && user.online === true) {
    appendToSystemMessage(`${userNameOrIdentity(user)} has connected.`);
  } else if (oldUser.online === true && user.online === false) {
    appendToSystemMessage(`${userNameOrIdentity(user)} has disconnected.`);
  }

  if (user.name !== oldUser.name) {
    appendToSystemMessage(
      `User ${userNameOrIdentity(oldUser)} renamed to ${userNameOrIdentity(user)}.`
    );
  }
});
```

### SetNameReducer.on callback - Handle errors and update profile name

We can also register callbacks to run each time a reducer is invoked. We register these callbacks using the `OnReducer` method which is automatically implemented for each reducer by `spacetime generate`.

Each reducer callback takes a number of parameters:

1. `ReducerEvent` that contains information about the reducer that triggered this event. It contains several fields. The ones we care about are:

   - `callerIdentity`: The `Identity` of the client that called the reducer.
   - `status`: The `Status` of the reducer run, one of `"Committed"`, `"Failed"` or `"OutOfEnergy"`.
   - `message`: The error message, if any, that the reducer returned.

2. The rest of the parameters are arguments passed to the reducer.

These callbacks will be invoked in one of two cases:

1. If the reducer was successful and altered any of our subscribed rows.
2. If we requested an invocation which failed.

Note that a status of `Failed` or `OutOfEnergy` implies that the caller identity is our own identity.

We already handle other users' `set_name` calls using our `User.onUpdate` callback, but we need some additional behavior for setting our own name. If our name was rejected, we'll update the `system` message. If our name was accepted, we'll update our name in the app.

We'll test both that our identity matches the sender and that the status is `Failed`, even though the latter implies the former, for demonstration purposes.

If the reducer status comes back as `committed`, we'll update the name in our app.

To the body of `App`, add:

```typescript
SetNameReducer.on((reducerEvent, newName) => {
  if (
    local_identity.current &&
    reducerEvent.callerIdentity.isEqual(local_identity.current)
  ) {
    if (reducerEvent.status === 'failed') {
      appendToSystemMessage(`Error setting name: ${reducerEvent.message} `);
    } else if (reducerEvent.status === 'committed') {
      setName(newName);
    }
  }
});
```

### SendMessageReducer.on callback - Handle errors

We handle warnings on rejected messages the same way as rejected names, though the types and the error message are different. We don't need to do anything for successful SendMessage reducer runs; our Message.onInsert callback already displays them.

To the body of `App`, add:

```typescript
SendMessageReducer.on((reducerEvent, newMessage) => {
  if (
    local_identity.current &&
    reducerEvent.callerIdentity.isEqual(local_identity.current)
  ) {
    if (reducerEvent.status === 'failed') {
      appendToSystemMessage(`Error sending message: ${reducerEvent.message} `);
    }
  }
});
```

## Update the UI button callbacks

We need to update the `onSubmitNewName` and `onMessageSubmit` callbacks to send the appropriate reducer to the module.

`spacetime generate` defined two functions for us, `SetNameReducer.call` and `SendMessageReducer.call`, which send a message to the database to invoke the corresponding reducer. The first argument, the `ReducerContext`, is supplied by the server, but we pass all other arguments ourselves. In our case, that means that both `SetNameReducer.call` and `SendMessageReducer.call` take one argument, a `String`.

Add the following to the `onSubmitNewName` callback:

```typescript
SetNameReducer.call(newName);
```

Add the following to the `onMessageSubmit` callback:

```typescript
SendMessageReducer.call(newMessage);
```

## Connecting to the module

We need to connect to the module when the app loads. We'll do this by adding a `useEffect` hook to the `App` function. This hook should only run once, when the component is mounted, but we are going to use an `initialized` boolean to ensure that it only runs once.

```typescript
useEffect(() => {
  if (!initialized.current) {
    client.current.connect();
    initialized.current = true;
  }
}, []);
```

## What's next?

When you run `npm start` you should see a chat app that can send and receive messages. If you open it in multiple private browser windows, you should see that messages are synchronized between them.

Congratulations! You've built a simple chat app with SpacetimeDB. You can find the full source code for this app [here](https://github.com/clockworklabs/spacetimedb-typescript-sdk/tree/main/examples/quickstart)

For a more advanced example of the SpacetimeDB TypeScript SDK, take a look at the [Spacetime MUD (multi-user dungeon)](https://github.com/clockworklabs/spacetime-mud/tree/main/react-client).

## Troubleshooting

If you encounter the following error:

```
TS2802: Type 'IterableIterator<any>' can only be iterated through when using the '--downlevelIteration' flag or with a '--target' of 'es2015' or higher.
```

You can fix it by changing your compiler target. Add the following to your `tsconfig.json` file:

```json
{
  "compilerOptions": {
    "target": "es2015"
  }
}
```
