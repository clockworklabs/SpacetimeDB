# @spacetimedb/auth

Authentication primitives for SpacetimeDB TypeScript modules. The package
provides password and OAuth handlers, ES256 sessions, connection binding,
profile management, and in-module rate limiting.

## Install

```bash
npm install @spacetimedb/auth spacetimedb@^2.8.3
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

The host module owns HTTP route registration and any mail-delivery adapter.

## Usage

### Integrate into an application

Import the submodule namespace, register the handlers your application needs,
then install Auth from the host `init` hook. Auth mounts and initializes its
Rate Limit dependency.

```ts
import { schema } from 'spacetimedb/server';
import * as auth from '@spacetimedb/auth/submodule';

const spacetimedb = schema({ auth });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  auth.installAuth(ctx.as.auth);
});
```

Register only the HTTP handlers and connection procedures your application
uses. For example, a login route calls `auth.passwordLoginHandler` with
`ctx.as.auth`; the host owns its router and trusted-proxy policy.

The complete wiring covers routes, connection binding, caller-scoped views,
and mail callbacks in the
[Auth example host module](./example/spacetimedb/).

Handlers set `Secure` cookies by default. Local HTTP examples pass
`{ secureCookies: false }` explicitly. To apply IP-based limits behind a proxy,
pass an `AuthHttpOptions` value naming the header that the proxy overwrites:

```ts
const authHttp = {
  trustedProxyHeader: 'x-forwarded-for',
} satisfies auth.AuthHttpOptions;

auth.passwordLoginHandler(ctx.as.auth, req, authHttp);
```

Register the handlers on the host router and expose authenticated application
operations through the connection binding:

```ts
import { Router } from 'spacetimedb/server';

export const authPasswordSignup = spacetimedb.httpHandler((ctx, req) =>
  auth.passwordSignupHandler(ctx.as.auth, req)
);

export const link_connection = spacetimedb.reducer(
  auth.linkConnectionParams,
  (ctx, args) => auth.link_connection(ctx.as.auth, args)
);

export const update_profile = spacetimedb.reducer(
  auth.updateProfileParams,
  (ctx, args) => auth.update_profile(ctx.as.auth, args)
);

export const router = spacetimedb.httpRouter(
  new Router().post('/auth/password/signup', authPasswordSignup)
);
```

The browser obtains a session through HTTP, binds its SpacetimeDB connection,
then calls normal generated operations:

```ts
const signup = await fetch('/auth/password/signup', {
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({ email, password, name }),
  credentials: 'same-origin',
});
if (!signup.ok) throw new Error(`signup_failed:${signup.status}`);
const { token } = (await signup.json()) as { token: string };

await conn.reducers.linkConnection({ sessionToken: token });
await conn.reducers.updateProfile({ name: 'Ada', image: undefined });
```

## API

The root entrypoint exports table builders, password and OAuth handlers, JWT
and key helpers, connection-binding procedures, and caller helpers. The
`./submodule` entrypoint exports the submodule schema, registered database
operations, views, handler factories, and `installAuth`. The host module owns
`init`, HTTP routing, cookie policy, and mail delivery.

Supported flows:

- Email/password signup and login
- ES256 JWT session cookies, refresh, logout, revoke, and sweep
- Google and GitHub OAuth through provider user-info endpoints
- Email verification and password-reset handler factories
- SpacetimeDB identity binding through `link_connection`
- Caller profile reads and updates
- Fixed-window limits for authentication endpoints

Submodule operations:

- Configuration and keys: `set_auth_config`, `get_auth_public_key`.
- Connection binding: `link_connection`, `unlink_connection`, and `whoami`.
- Profiles and sessions: `update_profile`, `list_my_sessions`,
  `revoke_my_session`, and administrative `revoke_session`.
- Caller helpers: `getCallerUserId` and the `my_auth_user` scoped view.

The handler exports are `passwordSignupHandler`, `passwordLoginHandler`,
`meHandler`, `refreshHandler`, `logoutHandler`, `googleStartHandler`,
`googleCallbackHandler`, `githubStartHandler`, `githubCallbackHandler`,
`makeForgotPasswordHandler`, `resetPasswordHandler`,
`makeEmailVerifyRequestHandler`, and `makeEmailVerifyHandler`.

Package entrypoints:

- `@spacetimedb/auth/submodule` is the normal host integration surface.
- `@spacetimedb/auth/handlers` exports HTTP handler factories.
- `@spacetimedb/auth/tables` exports lower-level table definitions.
- `@spacetimedb/auth/crypto`, `/jwt`, and `/keys` export focused helpers.
- `@spacetimedb/auth` re-exports the supported public surface.

## Security guarantees

- Passwords use scrypt with parameters encoded in the stored hash.
- Signing keys, OAuth secrets, and session state live in private tables.
- The publishing owner seeds the initial admin state during `init`.
- Authentication handlers use deterministic module context for time and
  randomness when running inside SpacetimeDB.
- Default authentication limits are production-oriented. Email-based limits
  work directly. IP-based limits and stored session IPs are
  enabled only when the host explicitly selects a trusted proxy header.

- Google honors the provider's `email_verified` claim. GitHub selects a
  verified address from the `/user/emails` response.
- When a new OAuth identity has the same email as an existing user, the callback
  returns `account_link_required`. The host can provide an authenticated account
  linking flow.
- OAuth completion redirects accept application-relative paths up to 2,048
  characters. Unsafe absolute, protocol-relative, backslash, fragment, control
  character, and encoded forms are rejected before state is stored.

Applications remain responsible for route exposure, cookie policy, mail
delivery, and the user experience for explicit account linking.

## Testing

```bash
pnpm test
pnpm run typecheck
```

The unit suite covers key generation, JWT validation, password hashing, PKCE,
tokens, and UUID generation. Build the example module to validate submodule
schema integration.

## License

[BUSL-1.1](./LICENSE.txt) - same as SpacetimeDB.
