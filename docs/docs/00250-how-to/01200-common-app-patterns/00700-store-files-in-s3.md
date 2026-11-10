---
title: Store files in S3
slug: /how-to/app-patterns/store-files-in-s3
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

:::note Prerequisites
You need a module and a client connected to it, and an Amazon S3 bucket, or a bucket on an S3-compatible service such as Cloudflare R2. If you don't have a module yet, follow the [quickstart](../../00100-intro/00100-getting-started/00100-getting-started.md) for your language first.
:::

In this guide, you'll let players upload files, list them, download them and delete them. The files go straight from the client to your bucket, so large files never pass through SpacetimeDB, and your database only keeps a record of each file. Your module signs short-lived URLs that each allow one kind of request on one file, so your S3 secret key never reaches the client. You'll sign them in [procedures](../../00200-core-concepts/00200-functions/00400-procedures.md), with the keys stored in [environment variables](../../00200-core-concepts/00100-databases/00700-environment-variables.md).

## How it works

- `S3_ACCESS_KEY_ID` and `S3_SECRET_ACCESS_KEY` environment variables store your S3 keys on the database.
- A `presign` function signs a URL for one kind of request on one file, such as an upload, valid for 5 minutes.
- A `request_upload` procedure checks the file's size, chooses where the file goes in the bucket, records it in a private `pending_upload` table, and returns a signed upload URL for a file of that size.
- The client uploads the file to that URL with an HTTP `PUT`.
- A `confirm_upload` procedure checks that the file is in the bucket and isn't too big, then moves it to the public `uploaded_file` table.
- A `request_download` procedure returns a signed download URL, and a `delete_file` procedure deletes a file from the bucket and from `uploaded_file`.
- Once an hour, a `clean_up_uploads` procedure deletes the uploads that players requested but never confirmed.

The bucket stays private: anyone who wants a file needs a URL that your module signed. The upload URL also fixes the file's size and content type, so the bucket rejects a file bigger than the one the player asked to upload.

## Set up the bucket

In your storage service:

- Create a bucket, and keep it private.
- Create an access key that can only list, read, write and delete files in that bucket. On AWS, that's an IAM user with the `s3:GetObject`, `s3:PutObject` and `s3:DeleteObject` permissions on `arn:aws:s3:::your-bucket/*`, and the `s3:ListBucket` permission on `arn:aws:s3:::your-bucket`. Without `s3:ListBucket`, S3 answers a request for a file that doesn't exist with an access error, so `confirm_upload` can't tell that the file was never uploaded.
- If your client runs in a web browser, add a CORS rule to the bucket that allows `PUT` and `GET` requests, and the `Content-Type` header, from your site's origin. Browsers block the upload otherwise.

## Define the tables and the keys

Declare the `S3_ACCESS_KEY_ID` and `S3_SECRET_ACCESS_KEY` environment variables. They're required, so SpacetimeDB refuses to publish the module without them.

Then add three tables:

- `pending_upload` is private. It has one row per upload that a player requested but hasn't confirmed yet, with the file's name and content type. Its primary key is the file's key: its path in the bucket.
- `uploaded_file` is public, so clients can subscribe to it and list the files. It has one row per uploaded file, with its owner, key, name, content type, size and upload time.
- `upload_cleanup` is the schedule table that runs `clean_up_uploads`, as described in [Clean up abandoned uploads](#clean-up-abandoned-uploads).

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { ScheduleAt, TimeDuration } from 'spacetimedb';
import { schema, table, t, type InferSchema, type ProcedureCtx } from 'spacetimedb/server';
import { hmac } from '@noble/hashes/hmac.js';
import { sha256 } from '@noble/hashes/sha2.js';
import { bytesToHex, utf8ToBytes } from '@noble/hashes/utils.js';

const pendingUpload = table(
  { name: 'pending_upload' },
  {
    key: t.string().primaryKey(),
    owner: t.identity(),
    name: t.string(),
    contentType: t.string(),
    requestedAt: t.timestamp(),
  }
);

const uploadedFile = table(
  { name: 'uploaded_file', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    owner: t.identity(),
    key: t.string(),
    name: t.string(),
    contentType: t.string(),
    size: t.u64(),
    uploadedAt: t.timestamp(),
  }
);

const uploadCleanup = table(
  { name: 'upload_cleanup' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

const spacetimedb = schema(
  { pendingUpload, uploadedFile, uploadCleanup },
  { env: { S3_ACCESS_KEY_ID: t.string(), S3_SECRET_ACCESS_KEY: t.string() } }
);
export default spacetimedb;

type Ctx = ProcedureCtx<InferSchema<typeof spacetimedb>>;
```

Add `@noble/hashes` to your module's dependencies, to compute the signatures:

```bash
npm install @noble/hashes
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
#pragma warning disable STDB_UNSTABLE
using System.Globalization;
using System.Text;
using Org.BouncyCastle.Crypto.Digests;
using Org.BouncyCastle.Crypto.Macs;
using Org.BouncyCastle.Crypto.Parameters;
using SpacetimeDB;

[SpacetimeDB.Env]
public partial struct ModuleEnv
{
    public string S3_ACCESS_KEY_ID;
    public string S3_SECRET_ACCESS_KEY;
}

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "PendingUpload")]
    public partial struct PendingUpload
    {
        [SpacetimeDB.PrimaryKey]
        public string Key;
        public Identity Owner;
        public string Name;
        public string ContentType;
        public Timestamp RequestedAt;
    }

    [SpacetimeDB.Table(Accessor = "UploadedFile", Public = true)]
    public partial struct UploadedFile
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        public Identity Owner;
        public string Key;
        public string Name;
        public string ContentType;
        public ulong Size;
        public Timestamp UploadedAt;
    }

    [SpacetimeDB.Table(Accessor = "UploadCleanup", Scheduled = "CleanUpUploads", ScheduledAt = "ScheduledAt")]
    public partial struct UploadCleanup
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
    }
}
```

Add the `BouncyCastle.Cryptography` package to your module, to compute the signatures. The .NET classes in `System.Security.Cryptography` don't work in modules, because modules run on WebAssembly:

```bash
dotnet add package BouncyCastle.Cryptography
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use spacetimedb::http::{Request, Timeout};
use spacetimedb::{procedure, reducer, table, Identity, ProcedureContext, ReducerContext, ScheduleAt, Table, Timestamp};
use std::time::Duration;

#[spacetimedb::env]
pub struct Env {
    pub S3_ACCESS_KEY_ID: String,
    pub S3_SECRET_ACCESS_KEY: String,
}

#[table(accessor = pending_upload)]
pub struct PendingUpload {
    #[primary_key]
    key: String,
    owner: Identity,
    name: String,
    content_type: String,
    requested_at: Timestamp,
}

#[table(accessor = uploaded_file, public)]
pub struct UploadedFile {
    #[primary_key]
    #[auto_inc]
    id: u64,
    owner: Identity,
    key: String,
    name: String,
    content_type: String,
    size: u64,
    uploaded_at: Timestamp,
}

#[table(accessor = upload_cleanup, scheduled(clean_up_uploads))]
pub struct UploadCleanup {
    #[primary_key]
    #[auto_inc]
    scheduled_id: u64,
    scheduled_at: ScheduleAt,
}
```

Add `hmac` and `sha2` to your module's `Cargo.toml`, to compute the signatures:

```toml
[dependencies]
hmac = "0.12"
sha2 = "0.10"
```

</TabItem>
</Tabs>

Set the keys when you publish, by putting them in the publishing command's environment:

```bash
S3_ACCESS_KEY_ID='your-access-key-id' S3_SECRET_ACCESS_KEY='your-secret-access-key' spacetime publish my-game
```

The keys stay in the database across later publishes, so you only pass them once. See [Environment Variables](../../00200-core-concepts/00100-databases/00700-environment-variables.md#supply-values-when-publishing) for other ways to supply them, and to replace them.

## Sign requests

Add a `presign` function that signs a URL for one kind of request, such as a `PUT` or a `GET`, on one file. Anyone who has the URL can make that request until it expires, 5 minutes later, without your keys. It uses [AWS Signature Version 4](https://docs.aws.amazon.com/AmazonS3/latest/API/sigv4-query-string-auth.html), which S3-compatible services also accept:

1. It builds the request's canonical form: the method, the file's path, the signature's parameters and the signed headers, one per line.
2. It derives a signing key from your secret key, the date, the region and the service, with HMAC-SHA256.
3. It signs the canonical request with that key, and adds the signature to the URL.

For an upload, the URL also requires the request to send that exact `Content-Type` header, and a body of exactly the size that the module signed. The bucket rejects any other upload.

Set the constants at the top to your bucket's. With AWS, the host includes the bucket's name, and the bucket path is empty. Services that put the bucket in the path instead, such as Cloudflare R2, take their own host, and the bucket path `/your-bucket`.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Your bucket. With AWS: '<bucket>.s3.<region>.amazonaws.com', and an empty path.
const S3_HOST = 'my-bucket.s3.us-east-1.amazonaws.com';
const S3_BUCKET_PATH = '';
const S3_REGION = 'us-east-1';
const URL_EXPIRY_SECONDS = 300; // 5 minutes

// Returns a URL that allows `method` requests on `key`, until it expires.
// For an upload, pass the content type and the size: the request must send that
// `Content-Type` header, and a body of exactly `contentLength` bytes.
function presign(ctx: Ctx, method: string, key: string, contentType = '', contentLength = 0n): string {
  // For example, 20260115T093000Z.
  const amzDate = ctx.timestamp.toDate().toISOString().replace(/[-:]/g, '').replace(/\.\d+/, '');
  const date = amzDate.slice(0, 8);
  const scope = `${date}/${S3_REGION}/s3/aws4_request`;
  const path = `${S3_BUCKET_PATH}/${key}`;
  const headers = contentType
    ? `content-length:${contentLength}\ncontent-type:${contentType}\nhost:${S3_HOST}\n`
    : `host:${S3_HOST}\n`;
  const signedHeaders = contentType ? 'content-length;content-type;host' : 'host';
  const query = [
    'X-Amz-Algorithm=AWS4-HMAC-SHA256',
    `X-Amz-Credential=${encodeURIComponent(`${ctx.env.S3_ACCESS_KEY_ID}/${scope}`)}`,
    `X-Amz-Date=${amzDate}`,
    `X-Amz-Expires=${URL_EXPIRY_SECONDS}`,
    `X-Amz-SignedHeaders=${encodeURIComponent(signedHeaders)}`,
  ].join('&');

  const canonicalRequest = [method, path, query, headers, signedHeaders, 'UNSIGNED-PAYLOAD'].join('\n');
  const stringToSign = [
    'AWS4-HMAC-SHA256',
    amzDate,
    scope,
    bytesToHex(sha256(utf8ToBytes(canonicalRequest))),
  ].join('\n');
  let signingKey = utf8ToBytes(`AWS4${ctx.env.S3_SECRET_ACCESS_KEY}`);
  for (const part of [date, S3_REGION, 's3', 'aws4_request']) {
    signingKey = hmac(sha256, signingKey, utf8ToBytes(part));
  }
  const signature = bytesToHex(hmac(sha256, signingKey, utf8ToBytes(stringToSign)));
  return `https://${S3_HOST}${path}?${query}&X-Amz-Signature=${signature}`;
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // Your bucket. With AWS: "<bucket>.s3.<region>.amazonaws.com", and an empty path.
    const string S3Host = "my-bucket.s3.us-east-1.amazonaws.com";
    const string S3BucketPath = "";
    const string S3Region = "us-east-1";
    const int UrlExpirySeconds = 300; // 5 minutes

    // Returns a URL that allows `method` requests on `key`, until it expires.
    // For an upload, pass the content type and the size: the request must send that
    // `Content-Type` header, and a body of exactly `contentLength` bytes.
    static string Presign(ProcedureContext ctx, string method, string key, string contentType = "", ulong contentLength = 0)
    {
        // For example, 20260115T093000Z.
        var amzDate = ((DateTimeOffset)ctx.Timestamp).UtcDateTime.ToString("yyyyMMdd'T'HHmmss'Z'", CultureInfo.InvariantCulture);
        var date = amzDate[..8];
        var scope = $"{date}/{S3Region}/s3/aws4_request";
        var path = $"{S3BucketPath}/{key}";
        var headers = contentType != ""
            ? $"content-length:{contentLength}\ncontent-type:{contentType}\nhost:{S3Host}\n"
            : $"host:{S3Host}\n";
        var signedHeaders = contentType != "" ? "content-length;content-type;host" : "host";
        var query = string.Join("&",
            "X-Amz-Algorithm=AWS4-HMAC-SHA256",
            $"X-Amz-Credential={Uri.EscapeDataString($"{ctx.Env.S3_ACCESS_KEY_ID}/{scope}")}",
            $"X-Amz-Date={amzDate}",
            $"X-Amz-Expires={UrlExpirySeconds}",
            $"X-Amz-SignedHeaders={Uri.EscapeDataString(signedHeaders)}");

        var canonicalRequest = string.Join("\n", method, path, query, headers, signedHeaders, "UNSIGNED-PAYLOAD");
        var stringToSign = string.Join("\n", "AWS4-HMAC-SHA256", amzDate, scope, Hex(Sha256(canonicalRequest)));
        var signingKey = Encoding.UTF8.GetBytes($"AWS4{ctx.Env.S3_SECRET_ACCESS_KEY}");
        foreach (var part in new[] { date, S3Region, "s3", "aws4_request" })
        {
            signingKey = HmacSha256(signingKey, part);
        }
        var signature = Hex(HmacSha256(signingKey, stringToSign));
        return $"https://{S3Host}{path}?{query}&X-Amz-Signature={signature}";
    }

    static byte[] Sha256(string data)
    {
        var bytes = Encoding.UTF8.GetBytes(data);
        var digest = new Sha256Digest();
        digest.BlockUpdate(bytes, 0, bytes.Length);
        var hash = new byte[digest.GetDigestSize()];
        digest.DoFinal(hash, 0);
        return hash;
    }

    static byte[] HmacSha256(byte[] key, string data)
    {
        var bytes = Encoding.UTF8.GetBytes(data);
        var mac = new HMac(new Sha256Digest());
        mac.Init(new KeyParameter(key));
        mac.BlockUpdate(bytes, 0, bytes.Length);
        var result = new byte[mac.GetMacSize()];
        mac.DoFinal(result, 0);
        return result;
    }

    static string Hex(byte[] bytes) => Convert.ToHexString(bytes).ToLowerInvariant();
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
/// Your bucket. With AWS: "<bucket>.s3.<region>.amazonaws.com", and an empty path.
const S3_HOST: &str = "my-bucket.s3.us-east-1.amazonaws.com";
const S3_BUCKET_PATH: &str = "";
const S3_REGION: &str = "us-east-1";
const URL_EXPIRY_SECONDS: u32 = 300; // 5 minutes

/// Returns a URL that allows `method` requests on `key`, until it expires.
/// For an upload, pass the content type and the size: the request must send that
/// `Content-Type` header, and a body of exactly `content_length` bytes.
fn presign(ctx: &ProcedureContext, method: &str, key: &str, content_type: &str, content_length: u64) -> String {
    // For example, 20260115T093000Z.
    let now = ctx.timestamp.to_rfc3339().expect("the time can be formatted");
    let amz_date = format!("{}T{}Z", now[..10].replace('-', ""), now[11..19].replace(':', ""));
    let date = &amz_date[..8];
    let scope = format!("{date}/{S3_REGION}/s3/aws4_request");
    let path = format!("{S3_BUCKET_PATH}/{key}");
    let (headers, signed_headers) = if content_type.is_empty() {
        (format!("host:{S3_HOST}\n"), "host")
    } else {
        (
            format!("content-length:{content_length}\ncontent-type:{content_type}\nhost:{S3_HOST}\n"),
            "content-length;content-type;host",
        )
    };
    let credential = format!("{}/{scope}", ctx.env.S3_ACCESS_KEY_ID());
    let query = [
        "X-Amz-Algorithm=AWS4-HMAC-SHA256".to_string(),
        format!("X-Amz-Credential={}", credential.replace('/', "%2F")),
        format!("X-Amz-Date={amz_date}"),
        format!("X-Amz-Expires={URL_EXPIRY_SECONDS}"),
        format!("X-Amz-SignedHeaders={}", signed_headers.replace(';', "%3B")),
    ]
    .join("&");

    let canonical_request = [method, &path, &query, &headers, signed_headers, "UNSIGNED-PAYLOAD"].join("\n");
    let string_to_sign = [
        "AWS4-HMAC-SHA256",
        &amz_date,
        &scope,
        &hex(&Sha256::digest(canonical_request.as_bytes())),
    ]
    .join("\n");
    let mut signing_key = format!("AWS4{}", ctx.env.S3_SECRET_ACCESS_KEY()).into_bytes();
    for part in [date, S3_REGION, "s3", "aws4_request"] {
        signing_key = hmac_sha256(&signing_key, part);
    }
    let signature = hex(&hmac_sha256(&signing_key, &string_to_sign));
    format!("https://{S3_HOST}{path}?{query}&X-Amz-Signature={signature}")
}

fn hmac_sha256(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts keys of any length");
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
```

</TabItem>
</Tabs>

The file keys that your module chooses only contain letters, digits and slashes, so `presign` doesn't need to URL-encode them.

## Request an upload

Add a `request_upload` procedure that clients call with the file's name, its content type, such as `image/png`, and its size in bytes. It:

- Rejects names that are empty or longer than 200 characters, content types that aren't a type and a subtype, such as `image/png`, and files bigger than 50 MB.
- Chooses the file's key from the caller's identity and the current time, such as `uploads/c200…/1767864600000000`. Players can't choose where their file goes, so they can't overwrite anyone else's.
- Records the upload in `pending_upload`, and returns the key and a signed `PUT` URL. The URL only accepts a file of that size and content type, so a player can't upload a bigger file than the one they asked for.

The procedure returns an `UploadTicket` value with three fields: `key`, `url`, and `error`, which explains what went wrong, or is empty on success. Returning errors this way, instead of throwing, means clients get the same kind of result whichever language your module is written in.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const MAX_NAME_LENGTH = 200;
const MAX_FILE_SIZE = 50 * 1024 * 1024; // 50 MB

const uploadTicket = t.object('UploadTicket', {
  key: t.string(),
  url: t.string(),
  error: t.string(),
});

export const requestUpload = spacetimedb.procedure(
  { name: t.string(), contentType: t.string(), size: t.u64() },
  uploadTicket,
  (ctx, { name, contentType, size }) => {
    if (name.length === 0 || name.length > MAX_NAME_LENGTH) {
      return { key: '', url: '', error: `File names must be 1 to ${MAX_NAME_LENGTH} characters long` };
    }
    if (!/^[\w.+-]+\/[\w.+-]+$/.test(contentType)) {
      return { key: '', url: '', error: "That doesn't look like a content type" };
    }
    if (size > BigInt(MAX_FILE_SIZE)) {
      return { key: '', url: '', error: `Files must be at most ${MAX_FILE_SIZE / (1024 * 1024)} MB` };
    }
    const key = `uploads/${ctx.sender.toHexString()}/${ctx.timestamp.microsSinceUnixEpoch}`;
    ctx.withTx(tx => {
      tx.db.pendingUpload.insert({ key, owner: tx.sender, name, contentType, requestedAt: tx.timestamp });
    });
    return { key, url: presign(ctx, 'PUT', key, contentType, size), error: '' };
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    const int MaxNameLength = 200;
    const ulong MaxFileSize = 50 * 1024 * 1024; // 50 MB

    [SpacetimeDB.Type]
    public partial struct UploadTicket
    {
        public string Key;
        public string Url;
        public string Error;
    }

    [SpacetimeDB.Procedure]
    public static UploadTicket RequestUpload(ProcedureContext ctx, string name, string contentType, ulong size)
    {
        if (name.Length == 0 || name.Length > MaxNameLength)
        {
            return new UploadTicket { Key = "", Url = "", Error = $"File names must be 1 to {MaxNameLength} characters long" };
        }
        if (!IsContentType(contentType))
        {
            return new UploadTicket { Key = "", Url = "", Error = "That doesn't look like a content type" };
        }
        if (size > MaxFileSize)
        {
            return new UploadTicket { Key = "", Url = "", Error = $"Files must be at most {MaxFileSize / (1024 * 1024)} MB" };
        }
        var key = $"uploads/{ctx.Sender.ToString().ToLowerInvariant()}/{ctx.Timestamp.MicrosecondsSinceUnixEpoch}";
        ctx.WithTx(tx => tx.Db.PendingUpload.Insert(new PendingUpload
        {
            Key = key,
            Owner = tx.Sender,
            Name = name,
            ContentType = contentType,
            RequestedAt = tx.Timestamp,
        }));
        return new UploadTicket { Key = key, Url = Presign(ctx, "PUT", key, contentType, size), Error = "" };
    }

    // True for a type and a subtype, such as "image/png".
    static bool IsContentType(string contentType)
    {
        var parts = contentType.Split('/');
        return parts.Length == 2 && parts.All(part =>
            part.Length > 0 && part.All(c => char.IsAsciiLetterOrDigit(c) || c is '_' or '.' or '+' or '-'));
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const MAX_NAME_LENGTH: usize = 200;
const MAX_FILE_SIZE: u64 = 50 * 1024 * 1024; // 50 MB

#[derive(spacetimedb::SpacetimeType)]
pub struct UploadTicket {
    pub key: String,
    pub url: String,
    pub error: String,
}

impl UploadTicket {
    fn error(error: String) -> Self {
        UploadTicket {
            key: String::new(),
            url: String::new(),
            error,
        }
    }
}

#[procedure]
pub fn request_upload(ctx: &mut ProcedureContext, name: String, content_type: String, size: u64) -> UploadTicket {
    if name.is_empty() || name.chars().count() > MAX_NAME_LENGTH {
        return UploadTicket::error(format!("File names must be 1 to {MAX_NAME_LENGTH} characters long"));
    }
    if !is_content_type(&content_type) {
        return UploadTicket::error("That doesn't look like a content type".to_string());
    }
    if size > MAX_FILE_SIZE {
        return UploadTicket::error(format!("Files must be at most {} MB", MAX_FILE_SIZE / (1024 * 1024)));
    }
    let key = format!(
        "uploads/{}/{}",
        ctx.sender().to_hex(),
        ctx.timestamp.to_micros_since_unix_epoch()
    );
    ctx.with_tx(|tx| {
        tx.db.pending_upload().insert(PendingUpload {
            key: key.clone(),
            owner: tx.sender(),
            name: name.clone(),
            content_type: content_type.clone(),
            requested_at: tx.timestamp,
        });
    });
    let url = presign(ctx, "PUT", &key, &content_type, size);
    UploadTicket {
        key,
        url,
        error: String::new(),
    }
}

/// True for a type and a subtype, such as "image/png".
fn is_content_type(content_type: &str) -> bool {
    let is_token = |part: &str| {
        !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || "_.+-".contains(c))
    };
    match content_type.split_once('/') {
        Some((kind, subtype)) => is_token(kind) && is_token(subtype),
        None => false,
    }
}
```

</TabItem>
</Tabs>

:::note Accepting only some types
To accept only some kinds of files, such as images, check the content type against a list in `request_upload`. The content type is signed into the upload URL, so the bucket rejects an upload with a different `Content-Type` header. It's still the type that the client says the file has: if you need to know what the file really contains, check its content after the upload.
:::

## Confirm the upload

Add an `s3_request` function (`s3Request` in TypeScript, `S3Request` in C#) that sends a signed request for a file to the bucket, and returns the status of the reply, or 0 if there was no reply. For a `HEAD` request, it also returns the file's size.

Then add a `confirm_upload` procedure that clients call once their upload is done. It:

- Rejects keys that aren't one of the caller's pending uploads.
- Sends a `HEAD` request for the file, which returns its size without downloading it. It rejects the upload if the file isn't in the bucket.
- As a second check, if the file is bigger than 50 MB, deletes it from the bucket and from `pending_upload`, and rejects the upload.
- Otherwise, moves the upload from `pending_upload` to `uploaded_file`, in one transaction.

It returns an error message, or an empty string on success.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Sends a signed request for `key` to the bucket.
// Returns the status of the reply, or 0 if there was no reply, and the size of the file.
function s3Request(ctx: Ctx, method: string, key: string) {
  try {
    const response = ctx.http.fetch(presign(ctx, method, key), {
      method,
      timeout: TimeDuration.fromMillis(10_000),
    });
    return { status: response.status, size: Number(response.headers.get('content-length') ?? 0) };
  } catch (error) {
    console.error(`Could not reach the bucket: ${error}`);
    return { status: 0, size: 0 };
  }
}

export const confirmUpload = spacetimedb.procedure({ key: t.string() }, t.string(), (ctx, { key }) => {
  const pending = ctx.withTx(tx => tx.db.pendingUpload.key.find(key));
  if (!pending || !pending.owner.isEqual(ctx.sender)) {
    return 'Upload not found';
  }
  const head = s3Request(ctx, 'HEAD', key);
  if (head.status !== 200) {
    return head.status === 404 ? 'The file was not uploaded' : 'Could not check the file';
  }
  if (head.size > MAX_FILE_SIZE) {
    s3Request(ctx, 'DELETE', key);
    ctx.withTx(tx => tx.db.pendingUpload.key.delete(key));
    return `Files must be at most ${MAX_FILE_SIZE / (1024 * 1024)} MB`;
  }
  ctx.withTx(tx => {
    // Do nothing if another call already confirmed this upload.
    if (tx.db.pendingUpload.key.delete(key)) {
      tx.db.uploadedFile.insert({
        id: 0n,
        owner: pending.owner,
        key,
        name: pending.name,
        contentType: pending.contentType,
        size: BigInt(head.size),
        uploadedAt: tx.timestamp,
      });
    }
  });
  return '';
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    // Sends a signed request for `key` to the bucket.
    // Returns the status of the reply, or 0 if there was no reply, and the size of the file.
    static (int Status, ulong Size) S3Request(ProcedureContext ctx, SpacetimeDB.HttpMethod method, string key)
    {
        var request = new HttpRequest
        {
            Method = method,
            Uri = Presign(ctx, method.Value, key),
            Timeout = TimeSpan.FromSeconds(10),
        };
        switch (ctx.Http.Send(request))
        {
            case Result<HttpResponse, HttpError>.OkR(var response):
                var length = response.Headers.FirstOrDefault(header =>
                    header.Name.Equals("content-length", StringComparison.OrdinalIgnoreCase));
                var size = length.Value is null ? 0 : ulong.Parse(Encoding.UTF8.GetString(length.Value));
                return (response.StatusCode, size);
            case Result<HttpResponse, HttpError>.ErrR(var error):
                Log.Error($"Could not reach the bucket: {error}");
                return (0, 0);
            default:
                return (0, 0);
        }
    }

    [SpacetimeDB.Procedure]
    public static string ConfirmUpload(ProcedureContext ctx, string key)
    {
        var pending = ctx.WithTx(tx => tx.Db.PendingUpload.Key.Find(key));
        if (pending is not PendingUpload upload || upload.Owner != ctx.Sender)
        {
            return "Upload not found";
        }
        var head = S3Request(ctx, SpacetimeDB.HttpMethod.Head, key);
        if (head.Status != 200)
        {
            return head.Status == 404 ? "The file was not uploaded" : "Could not check the file";
        }
        if (head.Size > MaxFileSize)
        {
            S3Request(ctx, SpacetimeDB.HttpMethod.Delete, key);
            ctx.WithTx(tx => tx.Db.PendingUpload.Key.Delete(key));
            return $"Files must be at most {MaxFileSize / (1024 * 1024)} MB";
        }
        ctx.WithTx(tx =>
        {
            // Do nothing if another call already confirmed this upload.
            if (!tx.Db.PendingUpload.Key.Delete(key))
            {
                return false;
            }
            tx.Db.UploadedFile.Insert(new UploadedFile
            {
                Owner = upload.Owner,
                Key = key,
                Name = upload.Name,
                ContentType = upload.ContentType,
                Size = head.Size,
                UploadedAt = tx.Timestamp,
            });
            return true;
        });
        return "";
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
/// Sends a signed request for `key` to the bucket.
/// Returns the status of the reply, or 0 if there was no reply, and the size of the file.
fn s3_request(ctx: &mut ProcedureContext, method: &str, key: &str) -> (u16, u64) {
    let request = Request::builder()
        .method(method)
        .uri(presign(ctx, method, key, "", 0))
        .extension(Timeout(Duration::from_secs(10).into()))
        .body(())
        .expect("the request is valid");
    match ctx.http.send(request) {
        Ok(response) => {
            let size = response
                .headers()
                .get("content-length")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok())
                .unwrap_or(0);
            (response.status().as_u16(), size)
        }
        Err(error) => {
            log::error!("Could not reach the bucket: {error:?}");
            (0, 0)
        }
    }
}

#[procedure]
pub fn confirm_upload(ctx: &mut ProcedureContext, key: String) -> String {
    let pending = ctx.with_tx(|tx| tx.db.pending_upload().key().find(&key));
    let Some(pending) = pending.filter(|pending| pending.owner == ctx.sender()) else {
        return "Upload not found".to_string();
    };
    let (status, size) = s3_request(ctx, "HEAD", &key);
    if status != 200 {
        return if status == 404 { "The file was not uploaded" } else { "Could not check the file" }.to_string();
    }
    if size > MAX_FILE_SIZE {
        s3_request(ctx, "DELETE", &key);
        ctx.with_tx(|tx| tx.db.pending_upload().key().delete(&key));
        return format!("Files must be at most {} MB", MAX_FILE_SIZE / (1024 * 1024));
    }
    ctx.with_tx(|tx| {
        // Do nothing if another call already confirmed this upload.
        if tx.db.pending_upload().key().delete(&key) {
            tx.db.uploaded_file().insert(UploadedFile {
                id: 0,
                owner: pending.owner,
                key: key.clone(),
                name: pending.name.clone(),
                content_type: pending.content_type.clone(),
                size,
                uploaded_at: tx.timestamp,
            });
        }
    });
    String::new()
}
```

</TabItem>
</Tabs>

Clients can only confirm their own uploads, and only with the size that the bucket reports, so the `uploaded_file` table only lists files that are really in the bucket.

Until the upload URL expires, 5 minutes after `request_upload`, the player can use it again. They can only send a file of the same size and type to the same key, so at most they replace their own file. If they delete the file in that time, they can also put it back in the bucket, without a row in `uploaded_file`.

## Download and delete files

Add two more procedures:

- `request_download` returns a signed `GET` URL for a file in `uploaded_file`. Any player can download any file, as with a public file list. To limit who can, check the caller first, as `delete_file` does.
- `delete_file` deletes a file from the bucket, then from `uploaded_file`. Only the file's owner can delete it. For other players, it answers as if the file didn't exist.

`request_download` returns a `DownloadLink` value, with a `url` and an `error`. `delete_file` returns an error message, or an empty string on success.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const downloadLink = t.object('DownloadLink', {
  url: t.string(),
  error: t.string(),
});

export const requestDownload = spacetimedb.procedure({ fileId: t.u64() }, downloadLink, (ctx, { fileId }) => {
  const row = ctx.withTx(tx => tx.db.uploadedFile.id.find(fileId));
  if (!row) {
    return { url: '', error: 'File not found' };
  }
  return { url: presign(ctx, 'GET', row.key), error: '' };
});

export const deleteFile = spacetimedb.procedure({ fileId: t.u64() }, t.string(), (ctx, { fileId }) => {
  const row = ctx.withTx(tx => tx.db.uploadedFile.id.find(fileId));
  if (!row || !row.owner.isEqual(ctx.sender)) {
    return 'File not found';
  }
  const { status } = s3Request(ctx, 'DELETE', row.key);
  if (status < 200 || status >= 300) {
    return 'Could not delete the file';
  }
  ctx.withTx(tx => tx.db.uploadedFile.id.delete(fileId));
  return '';
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    [SpacetimeDB.Type]
    public partial struct DownloadLink
    {
        public string Url;
        public string Error;
    }

    [SpacetimeDB.Procedure]
    public static DownloadLink RequestDownload(ProcedureContext ctx, ulong fileId)
    {
        if (ctx.WithTx(tx => tx.Db.UploadedFile.Id.Find(fileId)) is not UploadedFile row)
        {
            return new DownloadLink { Url = "", Error = "File not found" };
        }
        return new DownloadLink { Url = Presign(ctx, "GET", row.Key), Error = "" };
    }

    [SpacetimeDB.Procedure]
    public static string DeleteFile(ProcedureContext ctx, ulong fileId)
    {
        if (ctx.WithTx(tx => tx.Db.UploadedFile.Id.Find(fileId)) is not UploadedFile row || row.Owner != ctx.Sender)
        {
            return "File not found";
        }
        var (status, _) = S3Request(ctx, SpacetimeDB.HttpMethod.Delete, row.Key);
        if (status < 200 || status >= 300)
        {
            return "Could not delete the file";
        }
        ctx.WithTx(tx => tx.Db.UploadedFile.Id.Delete(fileId));
        return "";
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[derive(spacetimedb::SpacetimeType)]
pub struct DownloadLink {
    pub url: String,
    pub error: String,
}

#[procedure]
pub fn request_download(ctx: &mut ProcedureContext, file_id: u64) -> DownloadLink {
    match ctx.with_tx(|tx| tx.db.uploaded_file().id().find(file_id)) {
        Some(row) => DownloadLink {
            url: presign(ctx, "GET", &row.key, "", 0),
            error: String::new(),
        },
        None => DownloadLink {
            url: String::new(),
            error: "File not found".to_string(),
        },
    }
}

#[procedure]
pub fn delete_file(ctx: &mut ProcedureContext, file_id: u64) -> String {
    let row = ctx.with_tx(|tx| tx.db.uploaded_file().id().find(file_id));
    let Some(row) = row.filter(|row| row.owner == ctx.sender()) else {
        return "File not found".to_string();
    };
    let (status, _) = s3_request(ctx, "DELETE", &row.key);
    if !(200..300).contains(&status) {
        return "Could not delete the file".to_string();
    }
    ctx.with_tx(|tx| tx.db.uploaded_file().id().delete(file_id));
    String::new()
}
```

</TabItem>
</Tabs>

## Clean up abandoned uploads

A player can request an upload and never confirm it, for example because they closed the game halfway through. Its row stays in `pending_upload`, and its file, if they sent one, stays in the bucket.

Insert a row into `upload_cleanup` from the `init` reducer, so that SpacetimeDB calls `clean_up_uploads` every hour. Then add the `clean_up_uploads` procedure. It finds the pending uploads that were requested more than an hour ago, and for each of them:

1. Deletes its row from `pending_upload`. From then on, `confirm_upload` rejects it, so it can't list a file that's about to be deleted.
2. Deletes its file from the bucket. Deleting a file that was never uploaded succeeds too, so there's no need to check first.

An upload URL expires after 5 minutes, so an hour leaves plenty of time for a slow upload to finish and be confirmed.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
const CLEANUP_INTERVAL_MICROS = 60n * 60n * 1_000_000n; // 1 hour
const ABANDONED_AFTER_MICROS = 60n * 60n * 1_000_000n; // 1 hour

export const init = spacetimedb.init(ctx => {
  ctx.db.uploadCleanup.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(CLEANUP_INTERVAL_MICROS),
  });
});

export const cleanUpUploads = spacetimedb.procedure(
  { onSchedule: uploadCleanup },
  { cleanup: uploadCleanup.rowType },
  t.unit(),
  ctx => {
    const cutoff = ctx.timestamp.microsSinceUnixEpoch - ABANDONED_AFTER_MICROS;
    const abandoned = ctx.withTx(tx =>
      Array.from(tx.db.pendingUpload.iter())
        .filter(upload => upload.requestedAt.microsSinceUnixEpoch < cutoff)
        .map(upload => upload.key)
    );
    for (const key of abandoned) {
      if (ctx.withTx(tx => tx.db.pendingUpload.key.delete(key))) {
        s3Request(ctx, 'DELETE', key);
      }
    }
    return {};
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
public static partial class Module
{
    static readonly TimeSpan CleanupInterval = TimeSpan.FromHours(1);
    static readonly TimeSpan AbandonedAfter = TimeSpan.FromHours(1);

    [SpacetimeDB.Reducer(ReducerKind.Init)]
    public static void Init(ReducerContext ctx)
    {
        ctx.Db.UploadCleanup.Insert(new UploadCleanup
        {
            ScheduledId = 0,
            ScheduledAt = new ScheduleAt.Interval(CleanupInterval),
        });
    }

    [SpacetimeDB.Procedure]
    public static void CleanUpUploads(ProcedureContext ctx, UploadCleanup cleanup)
    {
        var cutoff = ctx.Timestamp.MicrosecondsSinceUnixEpoch - (long)AbandonedAfter.TotalMicroseconds;
        var abandoned = ctx.WithTx(tx => tx.Db.PendingUpload.Iter()
            .Where(upload => upload.RequestedAt.MicrosecondsSinceUnixEpoch < cutoff)
            .Select(upload => upload.Key)
            .ToList());
        foreach (var key in abandoned)
        {
            if (ctx.WithTx(tx => tx.Db.PendingUpload.Key.Delete(key)))
            {
                S3Request(ctx, SpacetimeDB.HttpMethod.Delete, key);
            }
        }
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
const CLEANUP_INTERVAL: Duration = Duration::from_secs(60 * 60); // 1 hour
const ABANDONED_AFTER: Duration = Duration::from_secs(60 * 60); // 1 hour

#[reducer(init)]
pub fn init(ctx: &ReducerContext) {
    ctx.db.upload_cleanup().insert(UploadCleanup {
        scheduled_id: 0,
        scheduled_at: ScheduleAt::Interval(CLEANUP_INTERVAL.into()),
    });
}

#[procedure]
pub fn clean_up_uploads(ctx: &mut ProcedureContext, _cleanup: UploadCleanup) {
    let cutoff = ctx.timestamp.to_micros_since_unix_epoch() - ABANDONED_AFTER.as_micros() as i64;
    let abandoned: Vec<String> = ctx.with_tx(|tx| {
        tx.db
            .pending_upload()
            .iter()
            .filter(|upload| upload.requested_at.to_micros_since_unix_epoch() < cutoff)
            .map(|upload| upload.key)
            .collect()
    });
    for key in abandoned {
        if ctx.with_tx(|tx| tx.db.pending_upload().key().delete(&key)) {
            s3_request(ctx, "DELETE", &key);
        }
    }
}
```

</TabItem>
</Tabs>

If the bucket can't be reached, `s3_request` (`s3Request` in TypeScript, `S3Request` in C#) logs the error, and the file stays in the bucket without a row. It no longer appears anywhere in your database, so it's harmless, but it still uses storage.

:::note
`init` only runs when the database is created. Publishing new code to an existing database doesn't run it again, so the cleanup won't start there. To start it, either republish with `--delete-data`, which erases all data, or insert the row from another reducer. Before inserting, check that `upload_cleanup` is empty: with two rows, `clean_up_uploads` would run twice as often.
:::

## Upload from the client

On the client, subscribe to `uploaded_file` to list the files. To upload a file:

1. Call `request_upload` with the file's name, content type and size.
2. Send the file to the returned URL with an HTTP `PUT`, and a `Content-Type` header with the same content type. HTTP libraries send the file's size in a `Content-Length` header themselves, and it must match the size you passed.
3. Call `confirm_upload` with the returned key.

To download a file, call `request_download`, and open or fetch the returned URL within 5 minutes.

<Tabs groupId="client-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { DbConnection, tables } from './module_bindings';

const conn = DbConnection.builder()
  .withUri('ws://localhost:3000')
  .withDatabaseName('my-game')
  .onConnect(ctx => {
    ctx.subscriptionBuilder().subscribe([tables.uploadedFile]);
  })
  .build();

function listFiles() {
  for (const file of conn.db.uploadedFile.iter()) {
    console.log(`${file.id}: ${file.name} (${file.size} bytes)`);
  }
}

// Uploads a file, such as one the player picked in a file input.
async function uploadFile(file: File) {
  const contentType = file.type || 'application/octet-stream';
  try {
    const ticket = await conn.procedures.requestUpload({ name: file.name, contentType, size: BigInt(file.size) });
    if (ticket.error) {
      console.error(`Could not upload the file: ${ticket.error}`);
      return;
    }
    const response = await fetch(ticket.url, {
      method: 'PUT',
      headers: { 'content-type': contentType },
      body: file,
    });
    if (!response.ok) {
      console.error(`The upload failed with status ${response.status}`);
      return;
    }
    const error = await conn.procedures.confirmUpload({ key: ticket.key });
    if (error) {
      console.error(`Could not upload the file: ${error}`);
    }
  } catch (error) {
    // The procedure call or the upload itself failed, for example because a browser blocked it.
    console.error(`Could not upload the file: ${error}`);
  }
}

// Returns a URL to download the file from, valid for 5 minutes.
async function downloadUrl(fileId: bigint) {
  try {
    const link = await conn.procedures.requestDownload({ fileId });
    if (link.error) {
      console.error(`Could not download the file: ${link.error}`);
      return undefined;
    }
    return link.url;
  } catch (error) {
    console.error(`Could not call the procedure: ${error}`);
    return undefined;
  }
}

async function deleteFile(fileId: bigint) {
  try {
    const error = await conn.procedures.deleteFile({ fileId });
    if (error) {
      console.error(`Could not delete the file: ${error}`);
    }
  } catch (error) {
    console.error(`Could not call the procedure: ${error}`);
  }
}
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using System;
using System.IO;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Threading.Tasks;
using SpacetimeDB;
using SpacetimeDB.Types;

var http = new HttpClient();

var conn = DbConnection.Builder()
    .WithUri("http://localhost:3000")
    .WithDatabaseName("my-game")
    .OnConnect((conn, identity, token) =>
    {
        conn.SubscriptionBuilder()
            .AddQuery(q => q.From.UploadedFile())
            .Subscribe();
    })
    .Build();

void ListFiles()
{
    foreach (var file in conn.Db.UploadedFile.Iter())
    {
        Console.WriteLine($"{file.Id}: {file.Name} ({file.Size} bytes)");
    }
}

// Uploads the file at `path`.
void UploadFile(string path, string contentType)
{
    var size = (ulong)new FileInfo(path).Length;
    conn.Procedures.RequestUpload(Path.GetFileName(path), contentType, size, (ctx, result) =>
    {
        if (!result.IsSuccess || result.Value is not UploadTicket ticket)
        {
            Console.WriteLine($"Could not call the procedure: {result.Error}");
        }
        else if (ticket.Error != "")
        {
            Console.WriteLine($"Could not upload the file: {ticket.Error}");
        }
        else
        {
            _ = PutAndConfirm(ticket, path, contentType);
        }
    });
}

async Task PutAndConfirm(UploadTicket ticket, string path, string contentType)
{
    HttpResponseMessage response;
    try
    {
        var content = new ByteArrayContent(await File.ReadAllBytesAsync(path));
        content.Headers.ContentType = new MediaTypeHeaderValue(contentType);
        response = await http.PutAsync(ticket.Url, content);
    }
    catch (Exception error) when (error is HttpRequestException or IOException or TaskCanceledException)
    {
        // The file couldn't be read, or the bucket couldn't be reached.
        Console.WriteLine($"The upload failed: {error.Message}");
        return;
    }
    if (!response.IsSuccessStatusCode)
    {
        Console.WriteLine($"The upload failed with status {(int)response.StatusCode}");
        return;
    }
    conn.Procedures.ConfirmUpload(ticket.Key, (ctx, result) =>
    {
        if (!result.IsSuccess)
        {
            Console.WriteLine($"Could not call the procedure: {result.Error}");
        }
        else if (result.Value != "")
        {
            Console.WriteLine($"Could not upload the file: {result.Value}");
        }
    });
}

// Calls `onUrl` with a URL to download the file from, valid for 5 minutes.
void RequestDownload(ulong fileId, Action<string> onUrl)
{
    conn.Procedures.RequestDownload(fileId, (ctx, result) =>
    {
        if (!result.IsSuccess || result.Value is not DownloadLink link)
        {
            Console.WriteLine($"Could not call the procedure: {result.Error}");
        }
        else if (link.Error != "")
        {
            Console.WriteLine($"Could not download the file: {link.Error}");
        }
        else
        {
            onUrl(link.Url);
        }
    });
}

void DeleteFile(ulong fileId)
{
    conn.Procedures.DeleteFile(fileId, (ctx, result) =>
    {
        if (!result.IsSuccess)
        {
            Console.WriteLine($"Could not call the procedure: {result.Error}");
        }
        else if (result.Value != "")
        {
            Console.WriteLine($"Could not delete the file: {result.Value}");
        }
    });
}
```

Remember to call `conn.FrameTick()` regularly, for example once per frame, so the callbacks run. In Unity, send the file with `UnityWebRequest.Put` instead of `HttpClient`.

</TabItem>
<TabItem value="rust" label="Rust">

```rust
mod module_bindings;
use module_bindings::*;
use spacetimedb_sdk::{DbContext, Table};
use std::path::Path;
use std::sync::Arc;

fn main() {
    let conn = Arc::new(
        DbConnection::builder()
            .with_uri("http://localhost:3000")
            .with_database_name("my-game")
            .on_connect(|ctx, _identity, _token| {
                ctx.subscription_builder().add_query(|q| q.from.uploaded_file()).subscribe();
            })
            .build()
            .expect("failed to connect"),
    );

    // Process messages from the database on a background thread.
    conn.run_threaded();

    // Your game loop runs here.
}

fn list_files(conn: &DbConnection) {
    for file in conn.db().uploaded_file().iter() {
        println!("{}: {} ({} bytes)", file.id, file.name, file.size);
    }
}

/// Uploads the file at `path`.
fn upload_file(conn: &Arc<DbConnection>, path: String, content_type: String) {
    let name = Path::new(&path).file_name().unwrap_or_default().to_string_lossy().into_owned();
    let size = match std::fs::metadata(&path) {
        Ok(metadata) => metadata.len(),
        Err(error) => return eprintln!("Could not read the file: {error}"),
    };
    let conn_for_upload = conn.clone();
    conn.procedures.request_upload_then(name, content_type.clone(), size, move |_ctx, result| {
        let ticket = match result {
            Ok(ticket) if ticket.error.is_empty() => ticket,
            Ok(ticket) => return eprintln!("Could not upload the file: {}", ticket.error),
            Err(error) => return eprintln!("Could not call the procedure: {error:?}"),
        };
        // Send the file on another thread, so the callback doesn't hold up the connection.
        std::thread::spawn(move || {
            let body = match std::fs::read(&path) {
                Ok(body) => body,
                Err(error) => return eprintln!("Could not read the file: {error}"),
            };
            if let Err(error) = ureq::put(&ticket.url).header("content-type", &content_type).send(&body[..]) {
                return eprintln!("The upload failed: {error}");
            }
            conn_for_upload.procedures.confirm_upload_then(ticket.key, |_ctx, result| match result {
                Ok(error) if error.is_empty() => {}
                Ok(error) => eprintln!("Could not upload the file: {error}"),
                Err(error) => eprintln!("Could not call the procedure: {error:?}"),
            });
        });
    });
}

/// Calls `on_url` with a URL to download the file from, valid for 5 minutes.
fn request_download(conn: &DbConnection, file_id: u64, on_url: impl FnOnce(String) + Send + 'static) {
    conn.procedures.request_download_then(file_id, move |_ctx, result| match result {
        Ok(link) if link.error.is_empty() => on_url(link.url),
        Ok(link) => eprintln!("Could not download the file: {}", link.error),
        Err(error) => eprintln!("Could not call the procedure: {error:?}"),
    });
}

fn delete_file(conn: &DbConnection, file_id: u64) {
    conn.procedures.delete_file_then(file_id, |_ctx, result| match result {
        Ok(error) if error.is_empty() => {}
        Ok(error) => eprintln!("Could not delete the file: {error}"),
        Err(error) => eprintln!("Could not call the procedure: {error:?}"),
    });
}
```

Add `ureq` to your client's `Cargo.toml`, to send the file:

```toml
[dependencies]
ureq = "3"
```

</TabItem>
</Tabs>

## What's next?

You now store players' files in a bucket, and keep track of them in your database:

- Clients upload and download files directly from the bucket, with URLs that your module signs and that expire after 5 minutes.
- Your module chooses where each file goes and how big it can be, and checks that it's in the bucket before it lists it.
- A scheduled procedure deletes the uploads that players never confirmed.
- The bucket stays private, and your S3 keys never leave the database.

To adapt it to your app:

- **Limit who can download.** Check the caller in `request_download`, for example to let only the owner download a file, or the owner's friends, as in [Build a friends or follow list](../01100-social/00200-friends-list.md). To also hide the file list, make `uploaded_file` private and give each player their own files through a [view](../../00200-core-concepts/00200-functions/00500-views.md).
- **Other S3-compatible services.** With Cloudflare R2, set the host to `<account-id>.r2.cloudflarestorage.com`, the bucket path to `/your-bucket`, and the region to `auto`.
- **Public files.** To serve files that anyone can see, such as avatars, make their part of the bucket public, or put a CDN in front of it, and store the public URL in `uploaded_file` instead of signing download URLs.

To learn more about the features used here, see [Procedures](../../00200-core-concepts/00200-functions/00400-procedures.md), [Environment Variables](../../00200-core-concepts/00100-databases/00700-environment-variables.md) and [File Storage](../../00200-core-concepts/00300-tables/00210-file-storage.md).

Next, you can [keep usernames unique and block offensive ones](../01100-social/00100-unique-usernames.md).
