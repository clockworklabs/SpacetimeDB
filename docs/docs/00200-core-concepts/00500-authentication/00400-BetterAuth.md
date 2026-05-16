---
title: Better Auth
---

import { StepByStep, Step, StepText, StepCode } from "@site/src/components/Steps";
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

This guide will walk you through integrating **Better Auth** authentication with
your **SpacetimeDB** React application. You will configure Better Auth as an
OIDC provider, obtain an ID token from Better Auth, and pass that token to your
SpacetimeDB connection.

## Prerequisites

We assume you have the following prerequisites in place:

- A working SpacetimeDB project. Follow our [React Quickstart Guide](../../00100-intro/00200-quickstarts/00100-react.md)
  if you need help setting this up.
- A Better Auth application with a working sign-in flow.
- A public URL for your Better Auth server. SpacetimeDB validates the token by
  fetching OIDC metadata from the token issuer, so the issuer URL must be
  reachable by the SpacetimeDB server.

## Getting started

<StepByStep>

<Step title="Install Better Auth OIDC packages">
<StepText>
Install the Better Auth OAuth provider plugin on your auth server and
`react-oidc-context` in your React application.
</StepText>
<StepCode>
<Tabs groupId="package-manager" defaultValue="NPM">
<TabItem value="NPM" label="NPM">
```bash
npm add better-auth @better-auth/oauth-provider react-oidc-context
```
</TabItem>
<TabItem value="Yarn" label="Yarn">
```bash
yarn add better-auth @better-auth/oauth-provider react-oidc-context
```
</TabItem>
<TabItem value="PNPM" label="PNPM">
```bash
pnpm add better-auth @better-auth/oauth-provider react-oidc-context
```
</TabItem>
<TabItem value="Bun" label="Bun">
```bash
bun add better-auth @better-auth/oauth-provider react-oidc-context
```
</TabItem>
</Tabs>
</StepCode>
</Step>

<Step title="Configure Better Auth as an OIDC provider">
<StepText>
Add the Better Auth JWT and OAuth provider plugins. The OAuth provider plugin
exposes OIDC metadata and authorization endpoints, while the JWT plugin signs
the ID token that SpacetimeDB will validate.

Use the exact issuer URL in every place below. If your Better Auth routes live
under `/api/auth`, your issuer is usually `https://your-domain.com/api/auth`.

After changing the Better Auth configuration, run the Better Auth migration or
schema generation command for your project so the OAuth provider tables are
created.
</StepText>
<StepCode>

```typescript
// auth.ts
import { betterAuth } from 'better-auth';
import { jwt } from 'better-auth/plugins';
import { oauthProvider } from '@better-auth/oauth-provider';

export const auth = betterAuth({
  // ... your existing Better Auth configuration

  // OAuth Provider mode uses its own token endpoint.
  disabledPaths: ['/token'],

  plugins: [
    jwt(),
    oauthProvider({
      loginPage: '/sign-in',
      consentPage: '/consent',
      scopes: ['openid', 'profile', 'email'],
    }),
  ],
});
```

</StepCode>
</Step>

<Step title="Expose Better Auth OIDC metadata">
<StepText>
SpacetimeDB validates external tokens by reading
`<issuer>/.well-known/openid-configuration` and then fetching the issuer's JWKS.
Expose the Better Auth metadata routes in your framework. The example below uses
Next.js route handlers; use the equivalent route mechanism for your framework.
</StepText>
<StepCode>

```typescript
// app/api/auth/.well-known/openid-configuration/route.ts
import { oauthProviderOpenIdConfigMetadata } from '@better-auth/oauth-provider';
import { auth } from '@/lib/auth';

export const GET = oauthProviderOpenIdConfigMetadata(auth);
```

```typescript
// app/.well-known/oauth-authorization-server/api/auth/route.ts
import { oauthProviderAuthServerMetadata } from '@better-auth/oauth-provider';
import { auth } from '@/lib/auth';

export const GET = oauthProviderAuthServerMetadata(auth);
```

</StepCode>
</Step>

<Step title="Create an OAuth client">
<StepText>
Create a public OAuth client for your React application and save its
`client_id`. The redirect URI must match your local or production React app URL.

Run this from a trusted server script or admin route, not from browser code.
</StepText>
<StepCode>

```typescript
const client = await auth.api.adminCreateOAuthClient({
  headers,
  body: {
    client_name: 'SpacetimeDB React App',
    redirect_uris: ['http://localhost:5173'],
    token_endpoint_auth_method: 'none',
    skip_consent: true,
  },
});

console.log(client.client_id);
```

</StepCode>
</Step>

<Step title="Wrap your React app with AuthProvider">
<StepText>
Configure `react-oidc-context` to authenticate against your Better Auth issuer.
The `authority` value must match the `iss` claim in the token.
</StepText>
<StepCode>

```tsx
// main.tsx
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { AuthProvider } from 'react-oidc-context';

import App from './App.tsx';

const oidcConfig = {
  authority: '<YOUR_BETTER_AUTH_ISSUER>',
  client_id: '<YOUR_BETTER_AUTH_CLIENT_ID>',
  redirect_uri: window.location.origin,
  post_logout_redirect_uri: window.location.origin,
  response_type: 'code',
  scope: 'openid profile email',
  automaticSilentRenew: true,
};

function onSigninCallback() {
  window.history.replaceState({}, document.title, window.location.pathname);
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <AuthProvider {...oidcConfig} onSigninCallback={onSigninCallback}>
      <App />
    </AuthProvider>
  </StrictMode>
);
```

</StepCode>
</Step>

<Step title="Pass the Better Auth ID token to SpacetimeDB">
<StepText>
Use the ID token from `react-oidc-context` as the SpacetimeDB authentication
token. When a user is not signed in yet, redirect them to Better Auth first.
</StepText>
<StepCode>

```tsx
// App.tsx
import { useEffect, useMemo } from 'react';
import { useAuth } from 'react-oidc-context';
import { Identity } from 'spacetimedb';
import { SpacetimeDBProvider } from 'spacetimedb/react';
import { DbConnection, ErrorContext } from './module_bindings';

const onConnect = (_conn: DbConnection, identity: Identity) => {
  console.log(
    'Connected to SpacetimeDB with identity:',
    identity.toHexString()
  );
};

const onDisconnect = () => {
  console.log('Disconnected from SpacetimeDB');
};

const onConnectError = (_ctx: ErrorContext, err: Error) => {
  console.log('Error connecting to SpacetimeDB:', err);
};

function SpacetimeApp({ token }: { token: string }) {
  const connectionBuilder = useMemo(() => {
    return DbConnection.builder()
      .withUri('<YOUR SPACETIMEDB URL>')
      .withDatabaseName('<YOUR SPACETIMEDB MODULE NAME>')
      .withToken(token)
      .onConnect(onConnect)
      .onDisconnect(onDisconnect)
      .onConnectError(onConnectError);
  }, [token]);

  return (
    <SpacetimeDBProvider connectionBuilder={connectionBuilder}>
      <div>
        <h1>SpacetimeDB React App</h1>
        <p>You can now use SpacetimeDB in your app with Better Auth.</p>
      </div>
    </SpacetimeDBProvider>
  );
}

export default function App() {
  const auth = useAuth();

  useEffect(() => {
    if (!auth.isLoading && !auth.isAuthenticated && !auth.activeNavigator) {
      auth.signinRedirect().catch(console.error);
    }
  }, [auth]);

  if (auth.isLoading || auth.activeNavigator) {
    return <p>Loading...</p>;
  }

  if (auth.error) {
    return <p>Authentication error: {auth.error.message}</p>;
  }

  const token = auth.user?.id_token;

  if (!auth.isAuthenticated || !token) {
    return <p>Redirecting to sign in...</p>;
  }

  return <SpacetimeApp token={token} />;
}
```

</StepCode>
</Step>

<Step title="Validate Better Auth claims in your module">
<StepText>
SpacetimeDB validates the token signature before your reducers run. Your module
should still restrict which issuers and audiences it accepts.
</StepText>
<StepCode>

```typescript
import { SenderError } from 'spacetimedb/server';

const BETTER_AUTH_ISSUER = '<YOUR_BETTER_AUTH_ISSUER>';
const BETTER_AUTH_CLIENT_ID = '<YOUR_BETTER_AUTH_CLIENT_ID>';

export const onConnect = spacetimedb.clientConnected(ctx => {
  const jwt = ctx.senderAuth.jwt;

  if (jwt == null) {
    throw new SenderError('Unauthorized: JWT is required to connect');
  }

  if (jwt.issuer !== BETTER_AUTH_ISSUER) {
    throw new SenderError(`Unauthorized: Invalid issuer ${jwt.issuer}`);
  }

  if (!jwt.audience.includes(BETTER_AUTH_CLIENT_ID)) {
    throw new SenderError(`Unauthorized: Invalid audience ${jwt.audience}`);
  }
});
```

</StepCode>
</Step>

</StepByStep>

You are now set up to use **Better Auth** authentication with SpacetimeDB. When
users open your React application, they will sign in through Better Auth, receive
an OIDC ID token, and connect to SpacetimeDB using that token.
