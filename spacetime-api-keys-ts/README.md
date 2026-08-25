# @spacetimedb/api-keys

Reusable SpacetimeDB submodule for server-to-server API keys.

The submodule owns API key lifecycle state: key creation, hashed secret storage,
verification, scope checks, revocation, rotation, usage audit rows, and
admin-gated views. Host apps own what scopes mean.

## Install

```bash
npm install @spacetimedb/api-keys @spacetimedb/crypto spacetimedb@^2.8.3
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

## Usage

### Integrate into an application

Mount the submodule in the host schema and install its private state from the
host lifecycle hook:

```ts
import { schema, SenderError, t } from 'spacetimedb/server';
import * as apiKeys from '@spacetimedb/api-keys/submodule';

const spacetimedb = schema({
  apiKeys,
});

export const init = spacetimedb.init(ctx => {
  apiKeys.installApiKeys(ctx.as.apiKeys);
});

export default spacetimedb;
```

The root package exports a standalone `init`. The `./submodule` entrypoint
leaves lifecycle ownership with the host module.

Next, wrap `verifyApiKey` in the host operation that needs bearer-token access;
the submodule validates key material and scopes, while the host decides what a
scope authorizes. The complete
[Colony host module](./example/spacetimedb/)
shows scoped HTTP routes and one-time key delivery.

Verification runs through a host wrapper so the application can apply request
limits and derive the action being authorized.

## API

`create_api_key` creates a key for the caller's SpacetimeDB identity and returns
the raw key once. Only the hash and lookup prefix are stored.

```ts
const result = await conn.procedures.createApiKey({
  name: 'Deploy Bot',
  scopesJson: JSON.stringify(['files:read', 'files:write']),
  metadataJson: JSON.stringify({ environment: 'prod' }),
  expiresInSeconds: 60 * 60 * 24 * 90,
  keyPrefix: 'stdb_live',
});

console.log(result.key);
```

`create_api_key_for_subject` lets a submodule admin create service keys for a
specific owner subject.

Key lifecycle operations:

- `rotate_api_key({ keyId, expiresInSeconds, keyPrefix })` replaces a
  caller-owned key and returns the new raw key once.
- `revoke_api_key({ keyId })` revokes a caller-owned key.
- `revoke_api_key_for_subject({ keyId, ownerSubject })` is the administrative
  equivalent.
- `sweep_api_key_usage({ maxAgeSeconds, maxRows })` removes a bounded audit
  batch.
- `createApiKey`, `rotateApiKey`, `revokeApiKey`, and `verifyApiKey` are host
  helper functions for mounted applications.
- `add_admin_identity({ identity })` and `remove_admin_identity({ identity })`
  manage the administrator allowlist.

Each owner may have up to 50 active, unexpired keys. Expiration may be set up to
10 years from creation.

## Verify in a host app

Mounted apps can use the transactional helper directly:

```ts
const result = apiKeys.verifyApiKey(ctx.as.apiKeys, {
  key: bearerToken,
  requiredScope: 'files:write',
  action: 'upload_file',
});

if (!result.allowed) {
  throw new SenderError(`unauthorized:${result.reason}`);
}
```

Scope matching supports exact scopes, `*`, and prefix wildcards like
`files:*`.

After verification, keep the authorized mutation in the same host transaction:

```ts
export const upload_with_api_key = spacetimedb.procedure(
  { apiKey: t.string(), path: t.string(), bytes: t.array(t.u8()) },
  t.u64(),
  (ctx, args) =>
    ctx.withTx(tx => {
      const access = apiKeys.verifyApiKey(tx.as.apiKeys, {
        key: args.apiKey,
        requiredScope: 'files:write',
        action: 'upload_file',
      });
      if (!access.allowed || !access.ownerSubject) {
        throw new SenderError('api_key.unauthorized');
      }
      return writeAuthorizedFile(
        tx,
        access.ownerSubject,
        args.path,
        args.bytes
      );
    })
);
```

The generated client calls the host wrapper:

```ts
const fileId = await conn.procedures.uploadWithApiKey({
  apiKey,
  path: '/reports/latest.json',
  bytes,
});
```

`writeAuthorizedFile` represents the host application's protected mutation. It
uses the verified subject from the key record as its owner.

Package entrypoints:

- `@spacetimedb/api-keys` supports a standalone API-key database.
- `@spacetimedb/api-keys/submodule` supplies the mountable namespace,
  helpers, operations, and views for host applications.

## Tables and views

Private tables:

- `api_key`: key hash, prefix, owner, scopes, status, expiration, timestamps.
- `api_key_admin_identity`: submodule admins.
- `api_key_usage`: audit rows for verification, creation, rotation, and
  revocation.

Public views:

- `my_api_keys`: up to 500 current-identity key summaries, with no hashes or raw keys.
- `api_keys_admin`: up to 200 recent key summaries for submodule admins.
- `api_key_usage_admin`: recent usage/audit rows for submodule admins.

`sweep_api_key_usage` lets an admin delete up to 1,000 audit rows older than a
chosen age. Schedule it from the host according to the application's retention
policy.

## Security model

- Raw keys are returned once. Persistent state contains the hash and lookup
  prefix.
- Stored hashes are SHA-256 of high-entropy random keys.
- A short key prefix is stored for lookup and display.
- Public views expose safe summaries and omit key hashes and raw secrets.
- The submodule validates scopes as strings; the host app defines their meaning.
- Audit rows cover recognized keys, including expired, revoked, and
  scope-denied keys. Malformed and unknown input is rejected before audit
  storage.

## Testing

```bash
pnpm test
pnpm run typecheck
```

The example module exercises issuance, verification, rotation, revocation, and
admin views.

## License

[BUSL-1.1](./LICENSE.txt) - same as SpacetimeDB.
