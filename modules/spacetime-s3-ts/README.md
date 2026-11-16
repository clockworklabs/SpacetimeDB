# @spacetimedb/s3

Let users upload and download private files stored in Amazon S3. Files go
straight between the browser and S3. SpacetimeDB keeps track of who owns each
file and whether it is ready.

Once a user calls `completeUpload` or `deleteFile`, the submodule keeps going
after a server restart or a failed S3 request. If a copy keeps failing, by
default it stops an hour after the upload window closes and deletes the file.
Your app decides who can upload, download, and delete each file.

## Install

Run this in your SpacetimeDB TypeScript module directory:

```bash
npm install @spacetimedb/s3
```

Requires SpacetimeDB SDK, CLI, and server 2.12 or later.

## Set up AWS

Create a general-purpose S3 bucket in a commercial region, with no dots in its
name, and with:

- All four Block Public Access settings on.
- Object Ownership set to **Bucket owner enforced**.
- Versioning off. It must never have been turned on.
- Default encryption SSE-S3.
- A prefix (folder) used only by this database, made of letters, digits, `_`, or
  `-` and ending in `/`. The submodule deletes files in its `final/` folder that
  the database doesn't know about, so restoring an old database backup deletes
  files uploaded since then.

Give the AWS user or role the module signs in as these permissions. Replace
`my-private-files` with your bucket and `my-app/` with your prefix:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "s3:GetBucketVersioning",
        "s3:GetBucketPublicAccessBlock",
        "s3:ListBucket"
      ],
      "Resource": "arn:aws:s3:::my-private-files"
    },
    {
      "Effect": "Allow",
      "Action": ["s3:GetObject", "s3:PutObject", "s3:DeleteObject"],
      "Resource": "arn:aws:s3:::my-private-files/my-app/*"
    }
  ]
}
```

Add a lifecycle rule that deletes unfinished uploads after two days:

```json
{
  "Rules": [
    {
      "ID": "expire-upload-staging",
      "Status": "Enabled",
      "Filter": { "Prefix": "my-app/temporary/" },
      "Expiration": { "Days": 2 }
    }
  ]
}
```

Allow your app's origin to upload and download:

```json
{
  "CORSRules": [
    {
      "AllowedOrigins": ["https://app.example.com"],
      "AllowedMethods": ["POST", "GET", "HEAD"],
      "AllowedHeaders": ["*"],
      "MaxAgeSeconds": 300
    }
  ]
}
```

Add a bucket policy that refuses requests without HTTPS:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Deny",
      "Principal": "*",
      "Action": "s3:*",
      "Resource": [
        "arn:aws:s3:::my-private-files",
        "arn:aws:s3:::my-private-files/*"
      ],
      "Condition": { "Bool": { "aws:SecureTransport": "false" } }
    }
  ]
}
```

## Integrate into an application

This module lets each user upload, download, and delete their own files. A user
here is a SpacetimeDB identity; if uploads need an account, check it in these
procedures:

```ts
import { ScheduleAt } from 'spacetimedb';
import {
  schema,
  t,
  type InferSchema,
  type ProcedureCtx,
  type TransactionCtx,
} from 'spacetimedb/server';
import * as s3 from '@spacetimedb/s3/submodule';
import {
  client,
  downloadGrant,
  s3Tick,
  uploadReservation,
} from '@spacetimedb/s3';

const spacetimedb = schema(
  { s3, s3Tick },
  {
    env: {
      AWS_ACCESS_KEY_ID: t.string(),
      AWS_SECRET_ACCESS_KEY: t.string(),
    },
  }
);
export default spacetimedb;

type Tx = TransactionCtx<InferSchema<typeof spacetimedb>>;
type Ctx = ProcedureCtx<InferSchema<typeof spacetimedb>>;

const files = client({
  submodule: (tx: Tx) => tx.as.s3,
  bucket: 'my-private-files',
  region: 'us-east-1',
  prefix: 'my-app/',
  accountId: '123456789012',
});

// Read both keys together, so replacing them can't mix an old key with a new one.
const credentials = (ctx: Ctx) =>
  ctx.withTx(({ env }) => ({
    accessKeyId: env.AWS_ACCESS_KEY_ID,
    secretAccessKey: env.AWS_SECRET_ACCESS_KEY,
  }));
// Each SpacetimeDB identity owns its own files.
const owner = (ctx: Ctx | Tx) => ctx.sender.toHexString();

export const init = spacetimedb.init(tx => {
  files.initialize(tx);
  tx.db.s3Tick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(5_000_000n), // every 5 seconds
  });
});

export const maintain = spacetimedb.procedure(
  { onSchedule: s3Tick },
  { tick: s3Tick.rowType },
  t.unit(),
  ctx => {
    files.reconcile(ctx, credentials(ctx));
    return {};
  }
);

export const beginUpload = spacetimedb.procedure(
  {
    requestKey: t.string(),
    filename: t.string(),
    contentType: t.string(),
    size: t.u32(),
    checksum: t.string(),
  },
  uploadReservation,
  (ctx, file) =>
    files.beginUpload(ctx, credentials(ctx), { ...file, owner: owner(ctx) })
);

export const completeUpload = spacetimedb.procedure(
  { id: t.string() },
  t.unit(),
  (ctx, { id }) => {
    files.completeUpload(ctx, credentials(ctx), { owner: owner(ctx), id });
    return {};
  }
);

export const getDownloadUrl = spacetimedb.procedure(
  { id: t.string() },
  downloadGrant,
  (ctx, { id }) =>
    files.getDownloadUrl(ctx, credentials(ctx), { owner: owner(ctx), id })
);

export const deleteFile = spacetimedb.reducer(
  { id: t.string() },
  (tx, { id }) => files.deleteFile(tx, { owner: owner(tx), id })
);
```

Publish with the AWS keys in your shell. They are stored privately in the
database:

```bash
AWS_ACCESS_KEY_ID='...' AWS_SECRET_ACCESS_KEY='...' spacetime publish my-app
```

To replace the keys later, set the new values and run
`spacetime publish my-app --env-only`. Temporary AWS credentials also work: pass
`sessionToken` and `expiresAt` (Unix milliseconds) and replace them before they
expire.

`init` only runs when a database is first created. To add the submodule to an
existing database, run the two calls inside `init` once, from a reducer only
your admins can call.

The browser uploads a file in four steps:

1. Calculate the file's SHA-256 checksum, encoded as base64.
2. Call `beginUpload`. Use a new request key for each file, and
   `application/octet-stream` if the browser reports no content type.
3. POST the returned `upload.fields` to `upload.url` as `FormData`, with the
   file added last as `file`. If this fails, call `deleteFile`.
4. Call `completeUpload`. The file becomes `Ready` once S3 has a checked copy.

Then `getDownloadUrl` returns a link that saves the file under its original
name. The submodule's tables are private; to list a user's files, add a view
that reads `ctx.db.s3.file.byOwnerState`. The
[browser example](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/spacetime-s3-ts/example)
has that view and the browser code.

Always set the owner on the server, as `owner` does above. Never take it from
the browser. Never send the AWS keys to a client, and don't log upload forms or
download links: anyone who has one can use it until it expires.

## API

`client(options)` takes the bucket, region, prefix, and `accountId` (the AWS
account that owns the bucket), plus these limits:

- `maxFileBytes`: largest file, 100 MiB by default, at most 1 GiB.
- `maxFilesPerOwner` and `maxBytesPerOwner`: per-user quota, 100 files and
  1 GiB by default. A file counts from `beginUpload` until it is deleted.
- `uploadSeconds`: time to upload and call `completeUpload`, 300 by default.
- `downloadSeconds`: how long a download link works, 60 by default.
- `httpTimeoutSeconds`: timeout for each request to S3, 15 by default.
- `recoverySeconds`, `receiptSeconds`, and `sweepSeconds`: how long to retry a
  failed copy after the upload window closes (1 hour), how long deleted files
  stay in the table (1 day), and how often to look for stray files in S3 (5
  minutes).

The bucket, region, prefix, and account ID are saved the first time `initialize`
runs. If you change them later, every call fails with
`s3.target_changed_or_not_initialized`, and moving to a new bucket or prefix
means migrating both the database and the files yourself.

It returns:

- `initialize(tx)` sets up the submodule. Call it from `init`.
- `beginUpload(ctx, credentials, { owner, requestKey, filename, contentType, size, checksum })`
  creates the file and returns its `id` and the `upload` form. Calling it again
  with the same request key returns the same file while it is in the table. If
  `upload` is missing, the file can't be uploaded again: use it if it is
  `Ready`, wait if it is `Copying`, and otherwise start again with a new key.
- `completeUpload(ctx, credentials, { owner, id })` copies the upload into place
  and checks it. If that fails, `reconcile` tries again.
- `getDownloadUrl(ctx, credentials, { owner, id })` returns a download link for
  a `Ready` file.
- `deleteFile(tx, { owner, id })` stops new download links at once; `reconcile`
  removes the file from S3.
- `reconcile(ctx, credentials)` finishes interrupted uploads and deletes, and
  removes files the database doesn't know about. Run it on a schedule.

A file's `state` is `Pending`, `Copying`, `Ready`, `Deleting`, or `Deleted`.
While `reconcile` retries a file, its `lastError` holds the latest error code.

Errors reach the caller as `s3.*` codes: `invalid_metadata`, `invalid_checksum`,
`request_key_conflict`, `quota_exceeded`, `upload_expired`, `file_not_found`,
`file_not_ready`, `invalid_config`, `invalid_credentials`,
`credentials_expiring`, `target_changed_or_not_initialized`, `versioned_bucket`,
and `public_access_not_blocked`. A failed S3 request reports
`s3.<method>_http_<status>`, such as `s3.get_http_403` for a missing permission,
or `s3.<method>_transport_failed`.

Package entrypoints:

- `@spacetimedb/s3/submodule` supplies the submodule's tables.
- `@spacetimedb/s3` exports `client`, `s3Tick`, `uploadReservation`,
  `downloadGrant`, `S3Error`, and their types.

## Testing

From `modules/spacetime-s3-ts`:

```bash
pnpm test
pnpm test:local
```

`test:local` needs the SpacetimeDB CLI on `PATH` and no AWS account. It runs a
real server and a fake S3 that fails on purpose, restarts the server partway
through uploads and deletes, and checks that every file ends up right.

## License

Apache-2.0. See the [license](https://github.com/clockworklabs/SpacetimeDB/blob/master/licenses/apache2.txt).
