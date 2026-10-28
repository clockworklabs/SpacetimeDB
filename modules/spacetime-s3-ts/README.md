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

## Setup

Mount the submodule and its maintenance table:

```ts
import {
  schema,
  type InferSchema,
  type TransactionCtx,
} from 'spacetimedb/server';
import * as s3 from '@spacetimedb/s3/submodule';
import { client, s3Tick } from '@spacetimedb/s3';

const db = schema({ s3, s3Tick });
export default db;

type Tx = TransactionCtx<InferSchema<typeof db>>;
const files = client({
  submodule: (tx: Tx) => tx.as.s3,
  bucket: 'my-private-files',
  region: 'us-east-1',
  prefix: 'my-app/',
  accountId: '123456789012',
});
```

Call `files.initialize(tx)` from your init reducer. Insert an interval into
`tx.db.s3Tick`, and register a scheduled procedure with `{ onSchedule: s3Tick }`
that calls `files.reconcile(ctx, credentials)`. A five-second interval is a useful
starting point. Keep that procedure private.

The [browser example](https://github.com/clockworklabs/SpacetimeDB/tree/master/modules/spacetime-s3-ts/example) has the complete wiring, typed environment
variables, caller checks, and an owner-filtered view.

## Upload and download

1. In the browser, calculate the file's SHA-256 checksum and encode it as base64.
2. Call your upload procedure. It calls `files.beginUpload(ctx, credentials, input)`
   with a request key, filename, content type, byte size, checksum, and owner.
3. Send every returned form field to `upload.url` in a `FormData` POST. Append the
   file last. S3 checks the exact size and checksum.
4. Call your completion procedure. It calls
   `files.completeUpload(ctx, credentials, { owner, id })`.
5. Wait for the file's state to become `Ready`. Then call
   `files.getDownloadUrl(ctx, credentials, { owner, id })` for a short-lived URL.

Derive `owner` on the server, for example from `ctx.sender.toHexString()`. For an
organization owner, check membership first. Do not accept an unchecked owner from
the browser. The submodule's tables are private; expose only the metadata each
caller may read through a host view.

A request key identifies one upload for one owner. Repeating it returns the same
file while its record exists. Reusing it with different metadata fails. A missing
`upload` grant means that this reservation is no longer open for upload.

To delete a file, call `files.deleteFile(tx, { owner, id })` from a reducer.
Maintenance removes its S3 objects and releases its quota. Keep the file ID until
the operation completes. Completion and deletion can be retried.

## AWS configuration

Use a dedicated AWS general-purpose bucket in a commercial region, with:

- All four S3 Block Public Access settings enabled.
- Object Ownership set to **Bucket owner enforced** (ACLs disabled).
- Versioning never enabled. Both enabled and suspended versioning are unsupported.
- SSE-S3 encryption (`AES256`). KMS and customer-provided encryption are unsupported.
- A dedicated prefix for this installation. Do not share it with another database
  or write objects into it outside this submodule.

The package uses regional AWS HTTPS endpoints. Dotted bucket names, custom
endpoints, access points, directory buckets, and multipart uploads are unsupported.
Bucket name, region, account ID, and prefix are stored at initialization. Changing
them requires an explicit data migration; a configuration edit cannot move files.

Give the module's IAM role these permissions. Replace the bucket and prefix:

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

Bucket-wide `ListBucket` lets S3 [return 404 for a missing object](https://docs.aws.amazon.com/AmazonS3/latest/API/API_HeadObject.html)
during a HEAD request. The submodule lists only its own prefix. Keep object read and write
permissions restricted to that prefix.

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

CORS does not grant access to private objects. Use a bucket policy that denies
non-TLS requests. Do not enable versioning, relax public access settings, or change
the bucket's encryption policy while this installation is active.

Supply credentials from trusted server configuration on every procedure call:

```ts
const credentials = {
  accessKeyId: ctx.env.AWS_ACCESS_KEY_ID,
  secretAccessKey: ctx.env.AWS_SECRET_ACCESS_KEY,
  sessionToken: ctx.env.AWS_SESSION_TOKEN,
  expiresAt: ctx.env.AWS_SESSION_EXPIRES_AT
    ? Number(ctx.env.AWS_SESSION_EXPIRES_AT)
    : undefined,
};
```

Use temporary credentials and rotate them before expiry. `expiresAt` is required
with a session token. Grants end before the session expires, with a 30-second
margin. The package does not log in to AWS or refresh credentials. Never return
credentials to a client or store them in public tables. Signed upload forms and
download URLs are bearer credentials: do not log them. A download URL remains
usable until it expires or its object is removed.

## Configuration

The defaults allow 100 files and 1 GiB per owner, with a 100 MiB limit per file.
Reservations count toward quota until deletion succeeds.

| Option               | Default | Purpose                                            |
| -------------------- | ------- | -------------------------------------------------- |
| `maxFileBytes`       | 100 MiB | Largest upload; maximum supported value is 1 GiB   |
| `maxFilesPerOwner`   | 100     | Active files and reservations per owner            |
| `maxBytesPerOwner`   | 1 GiB   | Reserved bytes per owner                           |
| `uploadSeconds`      | 300     | Time to upload and request completion              |
| `downloadSeconds`    | 60      | Download URL lifetime                              |
| `recoverySeconds`    | 3600    | Copy retry window after upload expiry              |
| `receiptSeconds`     | 86400   | Keep deleted records for request-key deduplication |
| `sweepSeconds`       | 300     | Delay between full orphan scans                    |
| `httpTimeoutSeconds` | 5       | Timeout for each server-to-S3 request              |

Maintenance handles one due file in each state per call. The orphan scan reads
one page of up to 16 keys per call and continues on the next call until it
reaches the end. Choose the maintenance interval and scan delay for your
workload. Check private `file.lastError` and `cleanup.lastError` through an
authorized admin path. They contain error codes, not credentials or signed URLs.

## Recovery

Completion first records its intent in SpacetimeDB. It then verifies the staging
object, copies it to a final key, and verifies that copy. Only then is the file
marked `Ready`. A retry checks for an already completed copy. A failed attempt
leaves work for maintenance, including after a server restart.

Deletion blocks new downloads at once. It then removes both keys. Late copy
results cannot restore a deleted record. The orphan scan removes final objects
that arrive after deletion; the lifecycle rule handles late staging uploads.

S3 and SpacetimeDB do not share a transaction. Cleanup is eventual. Keep the
maintenance schedule running and credentials valid. Restoring an old database
backup against a live prefix requires coordination: the orphan scan treats the
database as the source of truth and can delete newer S3 objects. Object keys
include the time the submodule was initialized, so a reset database never reuses
an earlier installation's keys; the orphan scan removes those older objects.

Server requests transfer metadata and copy instructions, not file bytes. The
SpacetimeDB HTTP client buffers responses before the package's XML size check.
Only AWS S3 endpoints are accepted.

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
