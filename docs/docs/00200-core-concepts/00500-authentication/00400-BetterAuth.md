---
title: Better Auth
---

import {
StepByStep,
Step,
StepText,
StepCode,
} from '@site/src/components/Steps';
import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

[Better Auth](https://www.better-auth.com/) is a TypeScript authentication
framework that can act as an OAuth 2.1/OIDC provider. SpacetimeDB can authenticate
Better Auth users when Better Auth issues a JWT with:

- a stable `iss` issuer,
- a stable `sub` subject,
- an `aud` audience you check in your module,
- and a JWKS endpoint SpacetimeDB can use to verify the token signature.

This guide shows the OAuth/OIDC provider pattern. Your application signs users in
with Better Auth, obtains an OIDC token, and passes that token to the SpacetimeDB
client connection.

::::warning
SpacetimeDB verifies JWTs through OIDC discovery and JWKS metadata. Opaque access
tokens cannot be validated this way. Make sure the token you pass to SpacetimeDB
is a JWT issued by Better Auth and signed by a key published in Better Auth's
JWKS.
::::

## Prerequisites

We assume you have the following prerequisites in place:

- A working SpacetimeDB project.
- A Better Auth application with a working sign-in flow.
- A public URL for your Better Auth server.
- An OAuth/OIDC client library for your frontend, backend, CLI, or native app.

SpacetimeDB validates the token by fetching OIDC metadata from the token issuer,
so the issuer URL must be reachable by the SpacetimeDB server.

## OAuth/OIDC flow overview

The integration has four parts:

1. Configure Better Auth as an OAuth/OIDC provider.
2. Publish Better Auth OIDC metadata and JWKS.
3. Create an OAuth client for the application that will connect to SpacetimeDB.
4. Obtain a Better Auth token and pass it to SpacetimeDB with `.withToken(...)`.

The examples below use placeholder URLs:

```txt
Better Auth issuer: https://app.example.com/api/auth
OAuth client ID:    <YOUR_BETTER_AUTH_CLIENT_ID>
SpacetimeDB URL:    <YOUR_SPACETIMEDB_URL>
Module name:        <YOUR_MODULE_NAME>
```

Use the exact same issuer value everywhere. The issuer must match the token's
`iss` claim and the OIDC discovery document's `issuer` field.

<StepByStep>

<Step title="Install Better Auth OIDC packages">
<StepText>
Install Better Auth and the OAuth Provider plugin on your auth server.

Your client application may use any OAuth/OIDC client library. For browser apps,
choose a library that supports Authorization Code with PKCE.

</StepText>
<StepCode>
<Tabs groupId="package-manager" defaultValue="NPM"> <TabItem value="NPM" label="NPM">

```bash
npm add better-auth @better-auth/oauth-provider
```

</TabItem>
<TabItem value="Yarn" label="Yarn">
```bash
yarn add better-auth @better-auth/oauth-provider
```
</TabItem>
<TabItem value="PNPM" label="PNPM">

```bash
pnpm add better-auth @better-auth/oauth-provider
```

</TabItem>
<TabItem value="Bun" label="Bun">

```bash
bun add better-auth @better-auth/oauth-provider
```

</TabItem>
</Tabs>
</StepCode>
</Step>

<Step title="Configure Better Auth as an OIDC provider">
<StepText>
Add the Better Auth JWT and OAuth Provider plugins.

The OAuth Provider plugin exposes the OAuth/OIDC authorization flow. The JWT
plugin signs the token that SpacetimeDB will validate.

For new integrations, prefer the OAuth Provider plugin over the older OIDC
Provider plugin.

</StepText>
<StepCode>

```typescript title="auth.ts"
import { betterAuth } from 'better-auth';
import { jwt } from 'better-auth/plugins';
import { oauthProvider } from '@better-auth/oauth-provider';

export const auth = betterAuth({
  // ... your existing Better Auth configuration

  // OAuth Provider mode uses its own token endpoint.
  disabledPaths: ['/token'],

  plugins: [
    jwt({
      jwks: {
        keyPairConfig: {
          // Prefer an asymmetric algorithm whose public keys can be published
          // through JWKS.
          alg: 'ES256',
        },
      },
    }),

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
SpacetimeDB validates external JWTs by reading:

```txt
<issuer>/.well-known/openid-configuration
```

It then follows the discovery document's `jwks_uri` to fetch the public signing
keys.

Expose the Better Auth metadata routes using your framework's routing mechanism.
The example below uses Next.js route handlers, but the same endpoints can be
served from any framework.

</StepText>
<StepCode>

```typescript title="app/api/auth/.well-known/openid-configuration/route.ts"
import { oauthProviderOpenIdConfigMetadata } from '@better-auth/oauth-provider';
import { auth } from '@/lib/auth';

export const GET = oauthProviderOpenIdConfigMetadata(auth);
```

```typescript title="app/.well-known/oauth-authorization-server/api/auth/route.ts"
import { oauthProviderAuthServerMetadata } from '@better-auth/oauth-provider';
import { auth } from '@/lib/auth';

export const GET = oauthProviderAuthServerMetadata(auth);
```

</StepCode>
</Step>

<Step title="Create an OAuth client">
<StepText>
Create an OAuth client for the application that will request the token.

For browser and native applications, use a public client with
`token_endpoint_auth_method: "none"` and Authorization Code with PKCE.

Run this from trusted server-side code, such as an admin script or admin route.
Do not create OAuth clients from browser code.

</StepText>
<StepCode>

```typescript
const client = await auth.api.adminCreateOAuthClient({
  headers,
  body: {
    client_name: 'SpacetimeDB App',
    redirect_uris: [
      'http://localhost:5173',
      'https://app.example.com/callback',
    ],
    token_endpoint_auth_method: 'none',
    skip_consent: true,
  },
});

console.log(client.client_id);
```

</StepCode>
</Step>

<Step title="Request an OIDC token">
<StepText>
Use your OAuth/OIDC client library to perform the Authorization Code with PKCE
flow.

The authorization request should use your Better Auth issuer and client ID, and
should request at least the `openid` scope.

The exact code depends on your framework and OAuth client library, but the
configuration usually looks like this:

</StepText>
<StepCode>

```typescript
const oidcConfig = {
  authority: 'https://app.example.com/api/auth',
  client_id: '<YOUR_BETTER_AUTH_CLIENT_ID>',
  redirect_uri: 'https://app.example.com/callback',

  response_type: 'code',
  scope: 'openid profile email',

  // Browser and native clients should use Authorization Code with PKCE.
  // Most OIDC client libraries enable PKCE automatically for public clients.
};
```

</StepCode>
</Step>

<Step title="Pass the Better Auth token to SpacetimeDB">
<StepText>
After the OAuth/OIDC flow completes, get the JWT from your OIDC client library
and pass it to SpacetimeDB with `.withToken(...)`.

For many OIDC clients, this token is exposed as `id_token`. Some OAuth Provider
flows may instead return a JWT access token. The important requirement is that
the token is a signed JWT whose `iss` matches your Better Auth issuer and whose
signing key is available through Better Auth's JWKS.

</StepText>
<StepCode>

```typescript
import { DbConnection } from './module_bindings';

const token = await getBetterAuthOidcToken();

const conn = DbConnection.builder()
  .withUri('<YOUR_SPACETIMEDB_URL>')
  .withDatabaseName('<YOUR_MODULE_NAME>')
  .withToken(token)
  .onConnect((_conn, identity) => {
    console.log(
      'Connected to SpacetimeDB with identity:',
      identity.toHexString()
    );
  })
  .onDisconnect(() => {
    console.log('Disconnected from SpacetimeDB');
  })
  .onConnectError((_ctx, err) => {
    console.error('Error connecting to SpacetimeDB:', err);
  })
  .build();
```

</StepCode>
</Step>

<Step title="Validate Better Auth claims in your module">
<StepText>
SpacetimeDB verifies the token signature before your reducers run. Your module
should still validate the claims that define your trust boundary.

At minimum, check:

- `iss`, to ensure the token came from your Better Auth issuer;
- `aud`, to ensure the token was meant for the expected client or resource;
- any custom claim your app uses for authorization, such as a tenant,
  organization, role, scope, or token type.

Do not treat a valid signature as the entire authorization decision.
</StepText>
<StepCode>

```typescript title="server/auth.ts"
import { SenderError } from 'spacetimedb/server';

const BETTER_AUTH_ISSUER = 'https://app.example.com/api/auth';
const BETTER_AUTH_CLIENT_ID = '<YOUR_BETTER_AUTH_CLIENT_ID>';

function stringClaim(
  payload: Record<string, unknown>,
  name: string
): string | undefined {
  const value = payload[name];
  return typeof value === 'string' ? value : undefined;
}

export const onConnect = spacetimedb.clientConnected(ctx => {
  const jwt = ctx.senderAuth.jwt;

  if (jwt == null) {
    throw new SenderError('Unauthorized: JWT is required to connect');
  }

  if (jwt.issuer !== BETTER_AUTH_ISSUER) {
    throw new SenderError('Unauthorized: invalid issuer');
  }

  if (!jwt.audience.includes(BETTER_AUTH_CLIENT_ID)) {
    throw new SenderError('Unauthorized: invalid audience');
  }

  // Optional: validate custom claims if your Better Auth token includes them.
  const tokenType = stringClaim(jwt.fullPayload, 'token_type');
  if (tokenType != null && tokenType !== 'spacetime-access') {
    throw new SenderError('Unauthorized: invalid token type');
  }

  // Store or refresh any connection/session state your reducers need for
  // module-local authorization decisions.
});
```

</StepCode>
</Step>

</StepByStep>

## Checklist

Before deploying, verify the following:

- The OIDC discovery document is available at
  `<issuer>/.well-known/openid-configuration`.
- The discovery document's `issuer` exactly matches the JWT `iss` claim.
- The discovery document's `jwks_uri` points to the JWKS containing the token
  signing key.
- The token you pass to SpacetimeDB is a JWT, not an opaque access token.
- The module checks `iss` and `aud` on connect.
- Any tenant, organization, role, scope, or permission claims are treated as
  authorization input, not as a replacement for reducer-level authorization.

You are now set up to use Better Auth authentication with SpacetimeDB. Your app
signs users in through Better Auth, receives an OIDC-compatible JWT, and connects
to SpacetimeDB using that token.
