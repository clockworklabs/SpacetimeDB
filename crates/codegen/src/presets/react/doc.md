# React Preset

This preset generates a React application with a Spacetime context provider and hooks for accessing the Spacetime client.

## Usage

Use the `--preset react` flag to generate the React bindings. This only works with the `typescript` language.

```bash
spacetime generate --lang typescript --out-dir module_bindings --project-path ./quickstart-chat/server  --preset react
```

This will generate the bindings in the `module_bindings` directory.


## Configuration

### Example

First, you need to import the `SpacetimeProvider` from the `react` module and encapsulate your application in it to provide the Spacetime client and hooks to your components.

```tsx
import { DbConnection } from './module_bindings';
import { SpacetimeProvider } from './module_bindings/react';

function App() {
  const subscribeToQueries = (conn: DbConnection) => {
    conn
      ?.subscriptionBuilder()
      .onApplied(() => {
        console.log('SDK client cache initialized.');
      })
      .subscribe(['SELECT * FROM message', 'SELECT * FROM user']);
  };

  return (
    <SpacetimeProvider
      builder={DbConnection.builder()
        .withUri('ws://localhost:3000')
        .withModuleName('quickstart-chat')
        .withToken(localStorage.getItem('auth_token') || '')}
      onConnect={subscribeToQueries}
    >
      <EnsureSpacetimeIsConnected>
        <h1>Hello, world!</h1>
      </EnsureSpacetimeIsConnected>
    </SpacetimeProvider>
  );
}

// This is a helper component to ensure that the Spacetime client is connected before rendering the children.
function EnsureSpacetimeIsConnected(props: React.PropsWithChildren) {
  const { client, identity, connected } = useSpacetimeContext();

  if (client && identity && connected) {
    return props.children;
  }

  return (
    <div className="App">
      <h1>Connecting...</h1>
    </div>
  );
}
```

Then, you can use the `useSpacetimeContext` hook to access the Spacetime client and hooks or use the generated hooks for each table to access the data.

```tsx
import { useSpacetimeUserStore } from './module_bindings/react';

function UserList() {
  const users = useSpacetimeUserStore();

  return (
    <div>
      {users.map((user) => <div key={user.id}>{user.name}</div>)}
    </div>
  );
}
```