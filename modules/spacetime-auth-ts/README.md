# @spacetimedb/auth

Add user accounts and sign-in to your SpacetimeDB application. Users can sign
in with a password, Google, or GitHub, manage their profile, and sign out of
individual sessions. The package also supports email verification and password
resets.

Use the signed-in user in your reducers, procedures, and HTTP routes to control
access to application data. Your application supplies its HTTP routes, signing
key, and email delivery service.

## Install

```bash
npm install @spacetimedb/auth spacetimedb
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

## Integrate into an application

Add Auth to your module and initialize it. This also sets up its built-in
rate limits:

```ts
import { schema } from 'spacetimedb/server';
import * as auth from '@spacetimedb/auth/submodule';

const spacetimedb = schema({ auth });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  auth.install(ctx.as.auth);
});
```

Choose the sign-in routes your application needs. The example below adds
password signup. Replace `deliver` with your email service. Set
`trustedProxyHeader` only when your server receives requests through a trusted
proxy that sets that header:

```ts
import { Router } from 'spacetimedb/server';

const authHttp = auth.client({
  trustedProxyHeader: 'x-forwarded-for',
  sendMail: (ctx, mail) => deliver(mail),
  appName: 'Notes',
});

export const authPasswordSignup = spacetimedb.httpHandler((ctx, req) =>
  authHttp.passwordSignup(ctx.as.auth, req)
);

export const router = spacetimedb.httpRouter(
  new Router().post('/auth/password/signup', authPasswordSignup)
);
```

The submodule registers its own reducers, procedures, and views under the
`auth` namespace, so the host does not wrap them. Host reducers read the caller
through the connection binding:

```ts
export const createNote = spacetimedb.reducer(
  { title: t.string() },
  (ctx, args) => {
    const userId = auth.requireCallerUserId(ctx.as.auth);
    // ...
  }
);
```

HTTP handlers have no caller identity. `auth.requestUserId(tx.as.auth, req)`
returns the user of the live session presented by the request's bearer token or
session cookie.

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

await conn.reducers['auth.linkConnection']({ sessionToken: token });
await conn.reducers['auth.updateProfile']({ name: 'Ada', image: undefined });
```

See the [Auth example](./example/) for sign-in, a log-based mailer,
and private notes. Before using the routes,
configure the signing key below and any OAuth credentials you need.

### Configuration and the signing key

An administrator calls `auth.set_auth_config` with the issuer URL, optional
OAuth client credentials, and, on the first call, an ES256 (P-256) PKCS#8
private key PEM. Later calls without a PEM keep the stored key. Generate the key
outside the module, for example:

```bash
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:P-256
```

The module runtime has no secure random source, so the key does two jobs: it
signs session JWTs, and it keys the HMAC that derives every session id, one-time
token, OAuth state, and password salt. Anyone holding it can mint sessions.

## API

`@spacetimedb/auth/submodule` is the host integration surface. It exports the
submodule schema, `install`, the registered operations, and the helpers below.
`@spacetimedb/auth` exports the helpers alone.

- `client(options)` returns the HTTP handlers: `passwordSignup`,
  `passwordLogin`, `me`, `refresh`, `logout`, `forgotPassword`,
  `resetPassword`, `emailVerifyRequest`, `emailVerify`, `googleStart`,
  `googleCallback`, `githubStart`, `githubCallback`, and `oauthStart(provider)`
  / `oauthCallback(provider)` for other providers.
- `getCallerUserId`, `requireCallerUserId`, and `findCallerUser` resolve the
  caller of a reducer, procedure, or view.
- `requestUserId` resolves the session presented to an HTTP handler.
- `errors` lists the `SenderError` codes.

`AuthHttpOptions`:

| Option                  | Default | Purpose                                                       |
| ----------------------- | ------- | ------------------------------------------------------------- |
| `trustedProxyHeader`    | none    | Enables IP limits and stored session IPs from a proxy header. |
| `secureCookies`         | `true`  | Set `false` only for local plain-HTTP development.            |
| `sendMail`              | none    | Delivers verification and reset mail.                         |
| `appName`               | none    | Names the application in outgoing mail.                       |
| `emailVerifiedRedirect` | `'/'`   | Where the email verification link lands.                      |

Registered operations:

- Configuration and keys: `set_auth_config`, `get_auth_public_key`.
- Administrators: `add_auth_admin`, `remove_auth_admin`.
- Connection binding: `link_connection`, `unlink_connection`, and `whoami`.
- Profiles and sessions: `update_profile`, `list_my_sessions`,
  `revoke_my_session`, and administrative `revoke_session`.
- The `my_auth_user` caller-scoped view.

## Security guarantees

- Passwords use scrypt with parameters encoded in the stored hash. Password
  checks run outside transactions, and unknown accounts are checked against a
  dummy hash so response time does not reveal which emails are registered.
- Tokens, IDs, and salts are HMAC-SHA256 outputs keyed by the signing key.
- Signing keys, OAuth secrets, and session state live in private tables.
- The publishing owner is the initial administrator. Only administrators change
  configuration, revoke other users' sessions, or change the administrator
  list, and the last administrator cannot be removed.
- A connection binding belongs to the session that created it. Revoking,
  expiring, or logging out a session removes its bindings; refreshing a
  session moves them to the new session. Password reset revokes every session.
- OAuth state is bound to the starting browser with an HttpOnly cookie.
- IP-based rate limits and stored session IPs require a trusted proxy header.
  Email-based rate limits are enabled by default.
- Google honors the provider's `email_verified` claim. GitHub selects a
  verified address from the `/user/emails` response.
- When a new OAuth identity has the same email as an existing user, the callback
  links it only if both the provider and the existing account have verified the
  address. Otherwise it returns `account_link_required`. Completing a password
  reset verifies the address, after which the OAuth sign-in links.
- OAuth completion redirects accept application-relative paths up to 2,048
  characters. Unsafe absolute, protocol-relative, backslash, fragment, control
  character, and encoded forms are rejected before state is stored.

## Testing

```bash
pnpm test
pnpm run typecheck
```

## License

Apache-2.0.
