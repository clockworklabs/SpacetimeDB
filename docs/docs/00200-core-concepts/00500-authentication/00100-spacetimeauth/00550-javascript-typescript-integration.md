---
title: JavaScript/TypeScript Integration
---

import { StepByStep, Step, StepText, StepCode } from "@site/src/components/Steps";
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

:::warning

SpacetimeAuth is currently in beta, some features may not be available yet or
may change in the future. You might encounter bugs or issues while using the
service. Please report any problems you encounter to help us improve SpacetimeAuth.

:::

This guide shows how to integrate SpacetimeAuth into a browser-based JavaScript
or TypeScript application without depending on a UI framework. It uses
[`oidc-client-ts`](https://github.com/authts/oidc-client-ts) to handle OpenID
Connect (OIDC), Authorization Code with PKCE, token storage, redirect callbacks,
and sign-out.

Use this guide when you are building with plain browser APIs or with a framework
that does not have a SpacetimeAuth-specific guide. If you are using React, see
the [React integration guide](./00500-react-integration.md).

## Prerequisites

1. Create a SpacetimeAuth project and configure a client as described in the
   [Creating a project](./00200-creating-a-project.md) and
   [Configuring your project](./00300-configuring-a-project.md) guides.
2. Configure your SpacetimeAuth client with your application's redirect URI and
   post-logout redirect URI. For local development, this might be
   `http://localhost:5173/auth/callback` and `http://localhost:5173`.
3. Have a browser-based JavaScript or TypeScript application that can run code on
   an auth callback route.

:::info

Do not expose a client secret in browser code. Browser applications should use a
public OIDC client and Authorization Code with PKCE.

:::

## Getting started

<StepByStep>
<Step title="Install oidc-client-ts">
<StepText>
Install `oidc-client-ts` in your client application.
</StepText>
<StepCode>
<Tabs groupId="package-manager" defaultValue="NPM">
<TabItem value="NPM" label="NPM">
```bash
npm add oidc-client-ts
```
</TabItem>
<TabItem value="Yarn" label="Yarn">
```bash
yarn add oidc-client-ts
```
</TabItem>
<TabItem value="PNPM" label="PNPM">
```bash
pnpm add oidc-client-ts
```
</TabItem>
<TabItem value="Bun" label="Bun">
```bash
bun add oidc-client-ts
```
</TabItem>
</Tabs>
</StepCode>
</Step>

<Step title="Create the OIDC client">
<StepText>
Create a small wrapper around `UserManager`. This keeps the OIDC protocol
handling inside `oidc-client-ts` and gives the rest of your application a simple
sign-in, callback, sign-out, and token API.

Replace `YOUR_CLIENT_ID` with the client ID from your SpacetimeAuth dashboard.
Set `redirect_uri` and `post_logout_redirect_uri` to URLs allowed by that client.
</StepText>
<StepCode>

```ts title="src/auth.ts"
import {
  UserManager,
  WebStorageStateStore,
  type User,
  type UserManagerSettings,
} from 'oidc-client-ts';

const oidcSettings: UserManagerSettings = {
  authority: 'https://auth.spacetimedb.com/oidc',
  client_id: 'YOUR_CLIENT_ID',
  redirect_uri: `${window.location.origin}/auth/callback`,
  post_logout_redirect_uri: window.location.origin,
  response_type: 'code',
  scope: 'openid profile email',
  automaticSilentRenew: true,
  userStore: new WebStorageStateStore({ store: window.localStorage }),
};

export const authClient = new UserManager(oidcSettings);

export async function signIn() {
  await authClient.signinRedirect();
}

export async function completeSignInCallback(): Promise<User> {
  const user = await authClient.signinRedirectCallback();
  window.history.replaceState({}, document.title, window.location.pathname);
  return user;
}

export async function signOut() {
  await authClient.signoutRedirect();
}

export async function getSignedInUser(): Promise<User | null> {
  const user = await authClient.getUser();
  return user && !user.expired ? user : null;
}

export async function getSpacetimeAuthToken(): Promise<string | undefined> {
  const user = await getSignedInUser();
  return user?.id_token;
}
```

</StepCode>
</Step>

<Step title="Handle the callback route">
<StepText>
On your callback route, let `oidc-client-ts` complete the redirect flow. After
that, route the user back to the part of your app that opens the SpacetimeDB
connection.

If your app uses a router, run this logic from the route handler for
`/auth/callback`. If your app is a single HTML page, check `window.location` at
startup.
</StepText>
<StepCode>

```ts title="src/auth-callback.ts"
import { completeSignInCallback } from './auth';

export async function handleAuthCallback() {
  try {
    await completeSignInCallback();
    window.location.assign('/');
  } catch (err) {
    console.error('Failed to complete SpacetimeAuth sign-in:', err);
    document.body.textContent = 'Authentication failed. Check the console.';
  }
}
```

```ts title="src/main.ts"
import { handleAuthCallback } from './auth-callback';
import { startApp } from './start-app';

if (window.location.pathname === '/auth/callback') {
  handleAuthCallback();
} else {
  startApp();
}
```

</StepCode>
</Step>

<Step title="Require sign-in before connecting">
<StepText>
Before opening the SpacetimeDB connection, get the current SpacetimeAuth ID
token. If no unexpired user is available, redirect the browser to SpacetimeAuth.
</StepText>
<StepCode>

```ts title="src/auth-token.ts"
import { getSpacetimeAuthToken, signIn } from './auth';

export async function requireSpacetimeAuthToken(): Promise<string> {
  const token = await getSpacetimeAuthToken();

  if (!token) {
    await signIn();
    throw new Error('Redirecting to SpacetimeAuth');
  }

  return token;
}
```

</StepCode>
</Step>

<Step title="Pass the token to SpacetimeDB">
<StepText>
Pass the SpacetimeAuth ID token to the generated TypeScript SDK connection
builder with `.withToken(...)`.

Replace `DbConnection` and `ErrorContext` imports with the path to your generated
module bindings. Replace the URI and database name with your SpacetimeDB host and
database name or identity.
</StepText>
<StepCode>

```ts title="src/spacetimedb.ts"
import { Identity } from 'spacetimedb';
import { DbConnection, ErrorContext } from './module_bindings';
import { requireSpacetimeAuthToken } from './auth-token';

let conn: DbConnection | undefined;

export async function connectToSpacetimeDB() {
  const token = await requireSpacetimeAuthToken();

  conn = DbConnection.builder()
    .withUri('ws://localhost:3000')
    .withDatabaseName('YOUR_DATABASE_NAME_OR_IDENTITY')
    .withToken(token)
    .onConnect((_conn, identity: Identity) => {
      console.log(
        'Connected to SpacetimeDB with identity:',
        identity.toHexString()
      );
    })
    .onDisconnect(() => {
      console.log('Disconnected from SpacetimeDB');
    })
    .onConnectError((_ctx: ErrorContext, err: Error) => {
      console.error('Error connecting to SpacetimeDB:', err);
    })
    .build();

  return conn;
}

export function disconnectFromSpacetimeDB() {
  conn?.disconnect();
  conn = undefined;
}
```

</StepCode>
</Step>

<Step title="Reconnect when the OIDC user changes">
<StepText>
`oidc-client-ts` can renew the OIDC user in the background. If your application
keeps a long-lived SpacetimeDB connection open, reconnect after the user changes
so the next connection uses the current token.
</StepText>
<StepCode>

```ts title="src/start-app.ts"
import { authClient, signOut } from './auth';
import {
  connectToSpacetimeDB,
  disconnectFromSpacetimeDB,
} from './spacetimedb';

export async function startApp() {
  await connectToSpacetimeDB();

  authClient.events.addUserLoaded(async () => {
    disconnectFromSpacetimeDB();
    await connectToSpacetimeDB();
  });

  authClient.events.addUserUnloaded(() => {
    disconnectFromSpacetimeDB();
  });

  document.querySelector('#sign-out')?.addEventListener('click', () => {
    disconnectFromSpacetimeDB();
    signOut();
  });
}
```

</StepCode>
</Step>
</StepByStep>

## Validate tokens in your module

SpacetimeDB validates the JWT signature before your reducers run. Your module
should still decide which issuers and audiences are allowed to connect. At a
minimum, check:

- `iss`, to ensure the token came from `https://auth.spacetimedb.com/oidc`;
- `aud`, to ensure the token was minted for your SpacetimeAuth client ID;
- any application-specific role or permission claims your module depends on.

See [Using Auth Claims](../00500-usage.md) for examples of reading and
validating claims in reducers.

## Notes

- Pass the ID token (`user.id_token`) to SpacetimeDB. Do not pass an opaque
  access token.
- The SpacetimeAuth issuer used by this guide is
  `https://auth.spacetimedb.com/oidc`.
- If your router stores callback state in a different URL, make sure that exact
  callback URL is configured in the SpacetimeAuth client.
- For production, use `wss://` for the SpacetimeDB URI and an HTTPS origin for
  your application.
