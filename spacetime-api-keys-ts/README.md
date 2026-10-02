# @spacetimedb/api-keys

Create and manage API keys for your SpacetimeDB application.

Create keys with permissions and an expiration time, check them before an
operation, and revoke or replace them when needed. Your application defines
what each permission allows. Copy each key when you create or replace it;
you cannot retrieve it later.

## Install

```bash
npm install @spacetimedb/api-keys spacetimedb
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

## Integrate into an application

Add API Keys to your module and initialize it:

```ts
import { schema, SenderError, t } from 'spacetimedb/server';
import * as apiKeys from '@spacetimedb/api-keys/submodule';

const spacetimedb = schema({
  apiKeys,
});

export const init = spacetimedb.init(ctx => {
  apiKeys.install(ctx.as.apiKeys);
});

export default spacetimedb;
```

`install` makes the publishing identity the first API key administrator and
schedules the usage sweep.

### Configure the secret

Before creating API keys, an administrator must configure a secret used to
generate them. Generate that secret outside the module and set it after
publishing:

```bash
spacetime call <database> api_keys.set_api_keys_config \
  "[0, \"$(openssl rand -hex 32)\"]" "[1, []]"
```

The secret must be at least 32 characters. Later calls may pass `none` for the
secret to keep the stored one. Until a secret is stored, key creation and
rotation fail with `api_keys.config_missing`.

### Verify keys in host operations

Check the API key in the operation it protects. This example requires the
`files:write` permission before uploading a file. Apply your request limits
before verification, and keep the key check and file write in the same
transaction:

```ts
export const uploadWithApiKey = spacetimedb.procedure(
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

`writeAuthorizedFile` represents the host application's protected mutation. It
uses the verified subject from the key record as its owner. The generated
client calls the host wrapper:

```ts
const fileId = await conn.procedures.uploadWithApiKey({
  apiKey,
  path: '/reports/latest.json',
  bytes,
});
```

Scope matching supports exact scopes, `*`, and prefix wildcards like
`files:*`. The complete [Colony host module](./example/spacetimedb/) shows
scoped HTTP routes and one-time key delivery.

## API

### Client operations

`create_api_key` creates a key for the caller's SpacetimeDB identity and returns
the raw key once. Only the hash and lookup prefix are stored. Deliver the key
to its holder directly; do not log it.

```ts
const { key } = await conn.procedures['apiKeys.createApiKey']({
  name: 'Deploy Bot',
  scopesJson: JSON.stringify(['files:read', 'files:write']),
  metadataJson: JSON.stringify({ environment: 'prod' }),
  expiresInSeconds: 60 * 60 * 24 * 90,
  keyPrefix: 'stdb_live',
});
```

- `rotate_api_key({ keyId, expiresInSeconds, keyPrefix })` replaces the secret
  of a caller-owned active, unexpired key and returns the new raw key once.
  Omitting `expiresInSeconds` keeps the current expiration.
- `revoke_api_key({ keyId })` revokes a caller-owned key.

### Administrator operations

- `create_api_key_for_subject` creates a key for a chosen owner subject.
- `revoke_api_key_for_subject({ keyId, ownerSubject })` revokes any key.
- `set_api_keys_config({ secret, usageRetentionSeconds })` stores the operator
  secret and how long usage rows are kept (default 30 days, at least one hour).
- `add_api_keys_admin({ identity })` and `remove_api_keys_admin({ identity })`
  manage the administrator allowlist. The last administrator cannot be
  removed.

### Host helpers

`createApiKeyInTx`, `rotateApiKeyInTx`, `revokeApiKeyInTx`, and `verifyApiKey`
take `tx.as.apiKeys` and run in the caller's transaction. Rotation and
revocation accept an `ownerSubject` so hosts can manage keys for application
user IDs. Every failure throws a code from the exported `errors` object.

Each owner may have up to 50 active, unexpired keys. Expiration may be set up to
10 years from creation.

Package entrypoints:

- `@spacetimedb/api-keys` exports `errors` and the host helpers.
- `@spacetimedb/api-keys/submodule` supplies the submodule namespace,
  `install`, helpers, operations, and views for host applications.
- [`spacetimedb/`](./spacetimedb/) publishes the submodule as a standalone
  database.

## Tables and views

Private tables:

- `api_key`: key hash, prefix, owner, scopes, status, expiration, timestamps.
- `api_key_admin_identity`: submodule admins.
- `api_key_config`: operator secret, derivation counter, and usage retention.
- `api_key_usage`: audit rows for verification, creation, rotation, and
  revocation.

Public views:

- `my_api_keys`: up to 500 current-identity key summaries, with no hashes or raw keys.
- `api_keys_admin`: up to 200 recent key summaries for submodule admins.
- `api_key_usage_admin`: recent usage/audit rows for submodule admins.

A scheduled sweep deletes up to 1,000 usage rows older than the retention
window every minute.

## Security model

- Key secrets are 256-bit HMAC-SHA256 outputs keyed by the operator secret
  over a stored counter and the transaction timestamp.
- Audit rows cover recognized keys, including wrong secrets for a known prefix
  and expired, revoked, and scope-denied keys. Malformed and unknown input is
  rejected before audit storage.

## Testing

```bash
pnpm test
pnpm run typecheck
```

## License

[Apache-2.0](./LICENSE.txt).
