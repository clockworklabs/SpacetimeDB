# @spacetimedb/s3

Store private files in Amazon S3. Keep their owners, metadata, and upload status in
SpacetimeDB. File bytes go straight between the browser and S3.

Use this submodule for user uploads that must survive a server restart or a failed
S3 request. Your app decides who can upload, download, and delete each file.

## Install

Run this in your SpacetimeDB TypeScript module directory:

```bash
npm install @spacetimedb/s3
```

Requires SpacetimeDB SDK, CLI, and server 2.12 or later.

## Quick start

This module lets each signed-in identity upload, download, and delete its own
files. Replace the bucket, region, prefix, and account ID with the values from
[AWS configuration](#aws-configuration).

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

// Read both keys from one snapshot, so a rotation never mixes old and new.
const credentials = (ctx: Ctx) =>
  ctx.withTx(({ env }) => ({
    accessKeyId: env.AWS_ACCESS_KEY_ID,
    secretAccessKey: env.AWS_SECRET_ACCESS_KEY,
  }));
// Each signed-in identity owns its own files.
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

Publish with the credentials in your shell. Publishing stores them in the
database's private environment:

```bash
AWS_ACCESS_KEY_ID='...' AWS_SECRET_ACCESS_KEY='...' spacetime publish my-app
```

To replace them later, set the new values and run
`spacetime publish my-app --env-only`. The next call uses them.

`init` runs only when a database is created. To add this submodule to an
existing database, make the same two calls once from a reducer that only your
administrators can call. Later `initialize` calls do nothing; insert the
schedule row only once.

The submodule's tables are private. To show each user their files, define a
view that reads `ctx.db.s3.file.byOwnerState` for the caller's owner key. The
[browser example](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/spacetime-s3-ts/example)
has that view and a browser client.

## Upload and download

1. In the browser, calculate the file's SHA-256 checksum and encode it as base64.
2. Call `beginUpload` with a request key, filename, content type, byte size, and
   checksum. The content type must be `type/subtype`; use
   `application/octet-stream` when the browser reports none.
3. Send every returned form field to `upload.url` in a `FormData` POST, then
   append the file as `file`. S3 checks the exact size and checksum and answers 204. If the POST fails, call `deleteFile` to release the reservation.
4. Call `completeUpload`. It returns once the copy is done or scheduled for a
   retry. The file's state then moves from `Copying` to `Ready`.
5. Once the file is `Ready`, call `getDownloadUrl`. Opening the URL saves the
   file under its original name.

Derive `owner` on the server, for example from `ctx.sender.toHexString()`. For an
organization owner, check membership first. Do not accept an unchecked owner from
the browser.

A request key identifies one upload for one owner. Repeating it returns the same
file while its record exists. Reusing it with different metadata fails. A missing
`upload` grant means that this reservation is no longer open for upload.

To delete a file, call `files.deleteFile(tx, { owner, id })` from a reducer.
New download URLs stop at once. Maintenance removes its S3 objects and releases
its quota. Completion and deletion can be retried.

## Files and states

Each upload is one row in the private `file` table: `id`, `owner`, `filename`,
`contentType`, `size`, `state`, `createdAt`, and `lastError`, among others. The
API passes `id` as a decimal string. The `byOwnerState` index covers
`(owner, state)`.

| State      | Meaning                                                                   |
| ---------- | ------------------------------------------------------------------------- |
| `Pending`  | Reserved. Waiting for the browser upload and `completeUpload`.            |
| `Copying`  | Completion recorded. The verified copy is running or waiting for a retry. |
| `Ready`    | Available for download.                                                   |
| `Deleting` | Downloads are blocked. Maintenance is removing its objects.               |
| `Deleted`  | Removed from S3. Kept for `receiptSeconds` to answer repeated requests.   |

A reservation that is not completed before its upload window closes is deleted.
A copy that keeps failing is retried for `recoverySeconds` after that, then
deleted. While maintenance retries, `lastError` holds the last error code.
Check it through an authorized admin path.

## Errors

Every error reaches the caller as its code.

| Code                                                  | Meaning                                                                        |
| ----------------------------------------------------- | ------------------------------------------------------------------------------ |
| `s3.invalid_metadata`                                 | The owner, request key, filename, content type, or size is not accepted.       |
| `s3.invalid_checksum`                                 | The checksum is not a base64 SHA-256 digest.                                   |
| `s3.request_key_conflict`                             | The request key was already used with different metadata.                      |
| `s3.quota_exceeded`                                   | The owner has reached the file or byte limit.                                  |
| `s3.upload_expired`                                   | The upload window closed before completion.                                    |
| `s3.file_not_found`                                   | The owner has no file with this ID.                                            |
| `s3.file_not_ready`                                   | The file is not `Ready`.                                                       |
| `s3.invalid_config`                                   | A `client()` option is outside its allowed format or range.                    |
| `s3.invalid_credentials`                              | A key is missing, or a session token has no `expiresAt`.                       |
| `s3.credentials_expiring`                             | The credentials expire in less than 31 seconds.                                |
| `s3.target_changed_or_not_initialized`                | `initialize` has not run, or the bucket, region, prefix, or account changed.   |
| `s3.versioned_bucket`, `s3.public_access_not_blocked` | The bucket does not meet the [AWS configuration](#aws-configuration).          |
| `s3.<method>_http_<status>`                           | S3 rejected a request, for example `s3.get_http_403` for a missing permission. |
| `s3.<method>_transport_failed`                        | A request did not reach S3 or timed out.                                       |

If the staged or copied object is missing or does not match, maintenance records
`s3.upload_missing`, `s3.object_mismatch`, `s3.copy_failed`, or `s3.copy_missing`
in `lastError` and retries.

## AWS configuration

Use a dedicated AWS general-purpose bucket in a commercial region, with:

- All four S3 Block Public Access settings enabled.
- Object Ownership set to **Bucket owner enforced** (ACLs disabled).
- Versioning never enabled. Both enabled and suspended versioning are unsupported.
- SSE-S3 encryption (`AES256`). KMS and customer-provided encryption are unsupported.
- A dedicated prefix for this installation. Do not share it with another database
  or write objects into it outside this submodule.

`accountId` is the AWS account that owns the bucket; every request checks it.
The prefix is one or more segments of letters, digits, `_`, or `-`, each ending
in `/`, up to 128 characters.

The package uses regional AWS HTTPS endpoints. Dotted bucket names, custom
endpoints, access points, directory buckets, and multipart uploads are unsupported.
Bucket name, region, account ID, and prefix are stored at initialization. Changing
them requires an explicit data migration; a configuration edit cannot move files.

Give these permissions to the IAM user or role whose credentials the module
uses. Replace the bucket and prefix:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": ["s3:GetBucketVersioning", "s3:GetBucketPublicAccessBlock"],
      "Resource": "arn:aws:s3:::my-private-files"
    },
    {
      "Effect": "Allow",
      "Action": "s3:ListBucket",
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

`ListBucket` lets S3 [return 404 for a missing object](https://docs.aws.amazon.com/AmazonS3/latest/API/API_HeadObject.html).
The submodule lists only its own prefix.

Set a lifecycle rule to expire `my-app/temporary/` objects after two days. These are
staging copies, including uploads that arrive after cancellation. Lifecycle
expiration is asynchronous. It is a required cleanup backstop, not a short timer.

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

For a browser app, configure bucket CORS for its exact origin:

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

CORS does not grant access to private objects. Add a bucket policy that denies
requests without TLS:

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

Keep these settings unchanged while this installation is active.

### Credentials

Pass credentials from trusted server configuration on every call, as the quick
start does. Use either an IAM user's access key limited to the policy above, or
temporary credentials with `sessionToken` and `expiresAt` in Unix milliseconds.
Temporary credentials must be replaced before they expire. Grants end 30 seconds
before `expiresAt`. The package does not log in to AWS or refresh credentials.

Never return credentials to a client or store them in public tables. Signed upload
forms and download URLs are bearer credentials: do not log them. A download URL
remains usable until it expires or its object is removed.

## Configuration

The defaults allow 100 files and 1 GiB per owner, with a 100 MiB limit per file.
Reservations count toward quota until deletion succeeds.

| Option               | Default | Allowed                 | Purpose                                            |
| -------------------- | ------- | ----------------------- | -------------------------------------------------- |
| `maxFileBytes`       | 100 MiB | up to 1 GiB             | Largest upload                                     |
| `maxFilesPerOwner`   | 100     | 1 to 10,000             | Active files and reservations per owner            |
| `maxBytesPerOwner`   | 1 GiB   | at least `maxFileBytes` | Reserved bytes per owner                           |
| `uploadSeconds`      | 300     | 1 to 3,600              | Time to upload and request completion              |
| `downloadSeconds`    | 60      | 1 to 3,600              | Download URL lifetime                              |
| `recoverySeconds`    | 3600    | 1 to 86,400             | Copy retry window after upload expiry              |
| `receiptSeconds`     | 86400   | 1 to 604,800            | Keep deleted records for request-key deduplication |
| `sweepSeconds`       | 300     | 1 to 86,400             | Delay between full orphan scans                    |
| `httpTimeoutSeconds` | 15      | 1 to 60                 | Timeout for each server-to-S3 request              |

Each maintenance call handles at most one due file in each state, so a
five-second interval processes up to 12 deletions a minute. The orphan scan
reads up to 16 keys per call and continues on the next call until it reaches
the end.

## Recovery

Completion records its intent in SpacetimeDB before it touches S3. It verifies
the staging object, copies it to a final key, and verifies that copy before the
file is `Ready`. A failed attempt leaves work for maintenance, including after a
server restart. Late copy results cannot restore a deleted record.

S3 and SpacetimeDB do not share a transaction, so cleanup is eventual. Keep the
maintenance schedule running and credentials valid. The orphan scan treats the
database as the source of truth for its prefix, so restoring an old backup
against a live prefix can delete newer objects. Object keys include the time the
submodule was initialized, so a reset database never reuses an earlier
installation's keys; the orphan scan removes those older objects.

## Tests

From the repository root:

```sh
pnpm --filter @spacetimedb/s3 test
pnpm --filter @spacetimedb/s3 test:local
```

The local test needs the SpacetimeDB CLI on `PATH` (or `SPACETIME_CLI`). It starts
its own server and fault-injection S3 receiver, tests real module transactions and
host restarts, and stops both afterward. It needs no AWS credentials. Live AWS
tests must use a separate private bucket and remove all test objects afterward.

## License

Apache-2.0. See the [license](https://github.com/clockworklabs/SpacetimeDB/blob/master/licenses/apache2.txt).
