# S3 example

A private file locker. Upload files to Amazon S3, see your own files in a live
list, download them, and delete them. File bytes go straight between the
browser and S3; SpacetimeDB keeps each file's owner, metadata, and upload state.

## Set up AWS

This example uses a real S3 bucket. Uploads and downloads are billed by AWS.

1. Create a bucket as described in
   [AWS configuration](../README.md#aws-configuration). Use the prefix
   `example/`, so the IAM policy resource ends in `example/*` and the lifecycle
   rule expires `example/temporary/`.
2. Add this CORS rule so the browser can upload and download:

   ```json
   {
     "CORSRules": [
       {
         "AllowedOrigins": ["http://127.0.0.1:8794"],
         "AllowedMethods": ["POST", "GET", "HEAD"],
         "AllowedHeaders": ["*"],
         "MaxAgeSeconds": 300
       }
     ]
   }
   ```

3. Create credentials with the IAM policy from the package README. A dedicated
   IAM user's access key works for local testing. For temporary credentials,
   also set `AWS_SESSION_TOKEN` and `AWS_SESSION_EXPIRES_AT`. AWS reports the
   expiry as an ISO time; convert it to Unix milliseconds, for example with
   `Date.parse(expiration)`.

## Run it locally

Requires Node.js 20+, pnpm 10, and the SpacetimeDB CLI and server built from
this checkout.

Start SpacetimeDB in a separate terminal:

```bash
spacetime start
```

The module reads its S3 settings and credentials from its environment. Set them
in the terminal you publish from; publishing stores them in the database. In
PowerShell:

```powershell
$env:S3_BUCKET = 'your-bucket'
$env:S3_REGION = 'us-east-1'
$env:S3_PREFIX = 'example/'
$env:S3_ACCOUNT_ID = '123456789012'
$env:AWS_ACCESS_KEY_ID = '...'
$env:AWS_SECRET_ACCESS_KEY = '...'
```

In bash, use `export S3_BUCKET=your-bucket` and so on.

From `modules/spacetime-s3-ts/example`, run:

```bash
pnpm install
pnpm run build:module
pnpm run dev
```

Open <http://127.0.0.1:8794>.

## Try it

1. Click **Upload** and choose a file. A progress bar shows the upload to S3. A
   spinner shows while the server checks and copies it. The download and delete
   buttons appear when it is done.
2. Click the download button. The browser saves the file under its original
   name.
3. Open the app in a private browser window. That window is a different user
   and sees only its own files.
4. Click the delete button. The file fades until maintenance removes it from
   S3, a few seconds later.

Each browser keeps its identity in local storage. Clearing site data creates a
new identity, and files owned by the old one are no longer reachable from that
browser.

## Change the example

- [spacetimedb/src/index.ts](./spacetimedb/src/index.ts) mounts the submodule,
  sets each file's owner to the caller's identity, and defines the file list
  view.
- [src/app.ts](./src/app.ts) hashes files, uploads them to S3, and handles the
  controls.

After changing the server code, run `pnpm run build:module`. This keeps existing
files and the stored S3 settings. Restart `pnpm run dev` after changing the
browser code.

To start over, run `pnpm run build:module:fresh`. **This deletes all data in the
local `spacetime-s3-example` database.** Maintenance then removes the old files
from S3.

For instructions on adding file uploads to your own app, see the
[S3 package README](../README.md).

## Configuration

See [.env.example](./.env.example) to change the database address or web-server
port. If you change the port, update the bucket's CORS origin to match.

To replace expired or rotated credentials, set the new variables and run:

```bash
spacetime publish --server local --env-only spacetime-s3-example
```

The bucket, region, prefix, and account ID are fixed when the database is
created. To use a different bucket, set the new values and run
`pnpm run build:module:fresh`.

## Before deploying

Anyone who opens this example gets an identity and can upload up to the default
quota of 100 files and 1 GiB. Connect owners to your app's user accounts, and
keep the AWS credentials limited to the package README policy.

## Troubleshooting

- **Upload fails with a network error:** check the bucket's CORS origin. It must
  match the address in the browser exactly, including the port.
- **A file keeps spinning:** check that the SpacetimeDB server is running and
  the credentials have not expired.
- **A faded file has only a delete button:** another tab is uploading it, or its
  upload stopped, for example because the page was reloaded. A stopped upload is
  removed when its upload window closes.
