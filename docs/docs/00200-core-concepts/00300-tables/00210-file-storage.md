---
title: File Storage
slug: /tables/file-storage
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


SpacetimeDB can store binary data directly in table columns, making it suitable for files, images, and other blobs that need to participate in transactions and subscriptions.

## Storing Binary Data Inline

Store binary data using `Vec<u8>` (Rust), `List<byte>` (C#), `std::vector<uint8_t>` (C++), or `t.array(t.u8())` (TypeScript). This approach keeps data within the database, ensuring it participates in transactions and real-time updates.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { table, t, schema } from 'spacetimedb/server';

const userAvatar = table(
  { name: 'user_avatar', public: true },
  {
    userId: t.u64().primaryKey(),
    mimeType: t.string(),
    data: t.array(t.u8()),  // Binary data stored inline
    uploadedAt: t.timestamp(),
  }
);

const spacetimedb = schema({ userAvatar });
export default spacetimedb;

export const upload_avatar = spacetimedb.reducer({
  userId: t.u64(),
  mimeType: t.string(),
  data: t.array(t.u8()),
}, (ctx, { userId, mimeType, data }) => {
  // Delete existing avatar if present
  ctx.db.userAvatar.userId.delete(userId);

  // Insert new avatar
  ctx.db.userAvatar.insert({
    userId,
    mimeType,
    data,
    uploadedAt: ctx.timestamp,
  });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "UserAvatar", Public = true)]
    public partial struct UserAvatar
    {
        [SpacetimeDB.PrimaryKey]
        public ulong UserId;
        public string MimeType;
        public List<byte> Data;  // Binary data stored inline
        public Timestamp UploadedAt;
    }

    [SpacetimeDB.Reducer]
    public static void UploadAvatar(ReducerContext ctx, ulong userId, string mimeType, List<byte> data)
    {
        // Delete existing avatar if present
        ctx.Db.UserAvatar.UserId.Delete(userId);

        // Insert new avatar
        ctx.Db.UserAvatar.Insert(new UserAvatar
        {
            UserId = userId,
            MimeType = mimeType,
            Data = data,
            UploadedAt = ctx.Timestamp,
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{ReducerContext, Timestamp, Table};

#[spacetimedb::table(accessor = user_avatar, public)]
pub struct UserAvatar {
    #[primary_key]
    user_id: u64,
    mime_type: String,
    data: Vec<u8>,  // Binary data stored inline
    uploaded_at: Timestamp,
}

#[spacetimedb::reducer]
pub fn upload_avatar(
    ctx: &ReducerContext,
    user_id: u64,
    mime_type: String,
    data: Vec<u8>,
) {
    // Delete existing avatar if present
    ctx.db.user_avatar().user_id().delete(user_id);

    // Insert new avatar
    ctx.db.user_avatar().insert(UserAvatar {
        user_id,
        mime_type,
        data,
        uploaded_at: ctx.timestamp,
    });
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
struct UserAvatar {
  uint64_t user_id;
  std::string mime_type;
  std::vector<uint8_t> data;  // Binary data stored inline
  Timestamp uploaded_at;
};
SPACETIMEDB_STRUCT(UserAvatar, user_id, mime_type, data, uploaded_at)
SPACETIMEDB_TABLE(UserAvatar, user_avatar, Public)
FIELD_PrimaryKey(user_avatar, user_id)

SPACETIMEDB_REDUCER(upload_avatar, ReducerContext ctx, 
  uint64_t user_id, std::string mime_type, std::vector<uint8_t> data) {
  // Delete existing avatar if present
  ctx.db[user_avatar_user_id].delete_by_key(user_id);

  // Insert new avatar
  ctx.db[user_avatar].insert(UserAvatar{
    .user_id = user_id,
    .mime_type = mime_type,
    .data = data,
    .uploaded_at = ctx.timestamp,
  });
    
  return Ok();
}
```

</TabItem>
</Tabs>

### When to Use Inline Storage

Inline storage works well for:

- **Files up to ~100MB**
- **Data that changes with other row fields** (e.g., user profile with avatar)
- **Data requiring transactional consistency** (file updates atomic with metadata)
- **Data clients need through subscriptions** (real-time avatar updates)

### Size Considerations

Very large binary data affects:

- **Memory usage**: Rows are held in memory during reducer execution
- **Network bandwidth**: Large rows increase subscription traffic
- **Transaction size**: Large rows slow down transaction commits

For very large files (over 100MB), consider external storage.

## External Storage with References

For large files or data that changes independently, store the file externally and keep a reference in the database. This pattern separates bulk storage from metadata management.

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { table, t, schema } from 'spacetimedb/server';

const document = table(
  { name: 'document', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    ownerId: t.identity().index('btree'),
    filename: t.string(),
    mimeType: t.string(),
    sizeBytes: t.u64(),
    storageUrl: t.string(),  // Reference to external storage
    uploadedAt: t.timestamp(),
  }
);

const spacetimedb = schema({ document });
export default spacetimedb;

// Called after uploading file to external storage
export const register_document = spacetimedb.reducer({
  filename: t.string(),
  mimeType: t.string(),
  sizeBytes: t.u64(),
  storageUrl: t.string(),
}, (ctx, { filename, mimeType, sizeBytes, storageUrl }) => {
  ctx.db.document.insert({
    id: 0,  // auto-increment
    ownerId: ctx.sender,
    filename,
    mimeType,
    sizeBytes,
    storageUrl,
    uploadedAt: ctx.timestamp,
  });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "Document", Public = true)]
    public partial struct Document
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public Identity OwnerId;
        public string Filename;
        public string MimeType;
        public ulong SizeBytes;
        public string StorageUrl;  // Reference to external storage
        public Timestamp UploadedAt;
    }

    // Called after uploading file to external storage
    [SpacetimeDB.Reducer]
    public static void RegisterDocument(
        ReducerContext ctx,
        string filename,
        string mimeType,
        ulong sizeBytes,
        string storageUrl)
    {
        ctx.Db.Document.Insert(new Document
        {
            Id = 0,  // auto-increment
            OwnerId = ctx.Sender,
            Filename = filename,
            MimeType = mimeType,
            SizeBytes = sizeBytes,
            StorageUrl = storageUrl,
            UploadedAt = ctx.Timestamp,
        });
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{Identity, ReducerContext, Timestamp, Table};

#[spacetimedb::table(accessor = document, public)]
pub struct Document {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    owner_id: Identity,
    filename: String,
    mime_type: String,
    size_bytes: u64,
    storage_url: String,  // Reference to external storage
    uploaded_at: Timestamp,
}

// Called after uploading file to external storage
#[spacetimedb::reducer]
pub fn register_document(
    ctx: &ReducerContext,
    filename: String,
    mime_type: String,
    size_bytes: u64,
    storage_url: String,
) {
    ctx.db.document().insert(Document {
        id: 0,  // auto-increment
        owner_id: ctx.sender(),
        filename,
        mime_type,
        size_bytes,
        storage_url,
        uploaded_at: ctx.timestamp,
    });
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
struct Document {
    uint64_t id;
    Identity owner_id;
    std::string filename;
    std::string mime_type;
    uint64_t size_bytes;
    std::string storage_url;  // Reference to external storage
    Timestamp uploaded_at;
};
SPACETIMEDB_STRUCT(Document, id, owner_id, filename, mime_type, size_bytes, storage_url, uploaded_at)
SPACETIMEDB_TABLE(Document, document, Public)
FIELD_PrimaryKeyAutoInc(document, id)
FIELD_Index(document, owner_id)

// Called after uploading file to external storage
SPACETIMEDB_REDUCER(register_document, ReducerContext ctx,
    std::string filename, std::string mime_type, uint64_t size_bytes, std::string storage_url) {
    ctx.db[document].insert(Document{
        .id = 0,  // auto-increment
        .owner_id = ctx.sender,
        .filename = filename,
        .mime_type = mime_type,
        .size_bytes = size_bytes,
        .storage_url = storage_url,
        .uploaded_at = ctx.timestamp,
    });
    
    return Ok();
}
```

</TabItem>
</Tabs>

### External Storage Options

Common external storage solutions include:

| Service | Use Case |
|---------|----------|
| AWS S3 / Google Cloud Storage / Azure Blob | General-purpose object storage |
| Cloudflare R2 | S3-compatible with no egress fees |
| CDN (CloudFront, Cloudflare) | Static assets with global distribution |
| Self-hosted (MinIO) | On-premises or custom deployments |

### Upload Flow

A typical external storage flow:

1. **Client requests upload URL**: Call a procedure or reducer to generate a pre-signed upload URL
2. **Client uploads directly**: Upload the file to external storage using the pre-signed URL
3. **Client registers metadata**: Call a reducer with the storage URL and file metadata
4. **Database tracks reference**: The table stores the URL for later retrieval

This pattern keeps large files out of SpacetimeDB while maintaining metadata in the database for queries and subscriptions.

### Example: Uploading to S3 from a Procedure

[Procedures](/functions/procedures) can make HTTP requests, enabling direct uploads to external storage services like S3. This example shows uploading a file to S3 and storing the metadata in SpacetimeDB:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { table, t, schema, SenderError } from 'spacetimedb/server';

const document = table(
  { name: 'document', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    ownerId: t.identity(),
    filename: t.string(),
    s3Key: t.string(),
    uploadedAt: t.timestamp(),
  }
);

const spacetimedb = schema({ document });
export default spacetimedb;

// Upload file to S3 and register in database
export const upload_to_s3 = spacetimedb.procedure(
  {
    filename: t.string(),
    contentType: t.string(),
    data: t.array(t.u8()),
    s3Bucket: t.string(),
    s3Region: t.string(),
  },
  t.string(),  // Returns the S3 key
  (ctx, { filename, contentType, data, s3Bucket, s3Region }) => {
    // Generate a unique S3 key
    const s3Key = `uploads/${Date.now()}-${filename}`;
    const url = `https://${s3Bucket}.s3.${s3Region}.amazonaws.com/${s3Key}`;

    // Upload to S3 (simplified - add AWS4 signature in production)
    const response = ctx.http.fetch(url, {
      method: 'PUT',
      headers: {
        'Content-Type': contentType,
        'x-amz-content-sha256': 'UNSIGNED-PAYLOAD',
        // Add Authorization header with AWS4 signature
      },
      body: new Uint8Array(data),
    });

    if (response.status !== 200) {
      throw new SenderError(`S3 upload failed: ${response.status}`);
    }

    // Store metadata in database
    ctx.withTx(txCtx => {
      txCtx.db.document.insert({
        id: 0n,
        ownerId: txCtx.sender,
        filename,
        s3Key,
        uploadedAt: txCtx.timestamp,
      });
    });

    return s3Key;
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
#pragma warning disable STDB_UNSTABLE
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Table(Accessor = "Document", Public = true)]
    public partial struct Document
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        public Identity OwnerId;
        public string Filename;
        public string S3Key;
        public Timestamp UploadedAt;
    }

    // Upload file to S3 and register in database
    [SpacetimeDB.Procedure]
    public static string UploadToS3(
        ProcedureContext ctx,
        string filename,
        string contentType,
        List<byte> data,
        string s3Bucket,
        string s3Region)
    {
        // Generate a unique S3 key
        var timestamp = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var s3Key = $"uploads/{timestamp}-{filename}";
        var url = $"https://{s3Bucket}.s3.{s3Region}.amazonaws.com/{s3Key}";

        // Build the S3 PUT request (simplified - add AWS4 signature in production)
        var request = new HttpRequest
        {
            Uri = url,
            Method = SpacetimeDB.HttpMethod.Put,
            Headers = new List<HttpHeader>
            {
                new HttpHeader("Content-Type", contentType),
                new HttpHeader("x-amz-content-sha256", "UNSIGNED-PAYLOAD"),
                // Add Authorization header with AWS4 signature
            },
            Body = new HttpBody(data.ToArray()),
        };

        // Upload to S3
        var response = ctx.Http.Send(request).UnwrapOrThrow();

        if (response.StatusCode != 200)
        {
            throw new Exception($"S3 upload failed with status: {response.StatusCode}");
        }

        // Store metadata in database
        ctx.WithTx(txCtx =>
        {
            txCtx.Db.Document.Insert(new Document
            {
                Id = 0,
                OwnerId = txCtx.Sender,
                Filename = filename,
                S3Key = s3Key,
                UploadedAt = txCtx.Timestamp,
            });
            return 0;
        });

        return s3Key;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{Identity, ProcedureContext, Timestamp, Table};

#[spacetimedb::table(accessor = document, public)]
pub struct Document {
    #[primary_key]
    #[auto_inc]
    id: u64,
    owner_id: Identity,
    filename: String,
    s3_key: String,
    uploaded_at: Timestamp,
}

// Upload file to S3 and register in database
#[spacetimedb::procedure]
pub fn upload_to_s3(
    ctx: &mut ProcedureContext,
    filename: String,
    content_type: String,
    data: Vec<u8>,
    s3_bucket: String,
    s3_region: String,
) -> Result<String, String> {
    // Generate a unique S3 key
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let s3_key = format!("uploads/{}-{}", timestamp, filename);
    let url = format!(
        "https://{}.s3.{}.amazonaws.com/{}",
        s3_bucket, s3_region, s3_key
    );

    // Build the S3 PUT request (simplified - add AWS4 signature in production)
    let request = spacetimedb::http::Request::builder()
        .uri(&url)
        .method("PUT")
        .header("Content-Type", &content_type)
        .header("x-amz-content-sha256", "UNSIGNED-PAYLOAD")
        // Add Authorization header with AWS4 signature
        .body(data)
        .map_err(|e| format!("Failed to build request: {}", e))?;

    // Upload to S3
    let response = ctx.http.send(request)
        .map_err(|e| format!("S3 upload failed: {:?}", e))?;

    let (parts, _body) = response.into_parts();
    if parts.status != 200 {
        return Err(format!("S3 upload failed with status: {}", parts.status));
    }

    // Store metadata in database
    let s3_key_clone = s3_key.clone();
    let filename_clone = filename.clone();
    ctx.with_tx(|tx_ctx| {
        tx_ctx.db.document().insert(Document {
            id: 0,
            owner_id: tx_ctx.sender,
            filename: filename_clone.clone(),
            s3_key: s3_key_clone.clone(),
            uploaded_at: tx_ctx.timestamp,
        });
    });

    Ok(s3_key)
}
```

</TabItem>
</Tabs>

:::note AWS Authentication
The example above is simplified. Production S3 uploads require proper [AWS Signature Version 4](https://docs.aws.amazon.com/AmazonS3/latest/API/sig-v4-authenticating-requests.html) authentication.
:::

### Alternative: Pre-signed URL Flow

For larger files, generate a pre-signed URL and let the client upload directly:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
// Procedure returns a pre-signed URL for client-side upload
export const get_upload_url = spacetimedb.procedure(
  { filename: t.string(), contentType: t.string() },
  t.object('UploadInfo', { uploadUrl: t.string(), s3Key: t.string() }),
  (ctx, { filename, contentType }) => {
    const s3Key = `uploads/${Date.now()}-${filename}`;

    // Generate pre-signed URL (requires AWS credentials and signing logic)
    const uploadUrl = generatePresignedUrl(s3Key, contentType);

    return { uploadUrl, s3Key };
  }
);

// Client uploads directly to S3 using the pre-signed URL, then calls:
export const confirm_upload = spacetimedb.reducer({ filename: t.string(), s3Key: t.string() }, (ctx, { filename, s3Key }) => {
  ctx.db.document.insert({
    id: 0n,
    ownerId: ctx.sender,
    filename,
    s3Key,
    uploadedAt: ctx.timestamp,
  });
});
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
#pragma warning disable STDB_UNSTABLE
using SpacetimeDB;

public static partial class Module
{
    [SpacetimeDB.Type]
    public partial struct UploadInfo
    {
        public string UploadUrl;
        public string S3Key;
    }

    // Procedure returns a pre-signed URL for client-side upload
    [SpacetimeDB.Procedure]
    public static UploadInfo GetUploadUrl(
        ProcedureContext ctx,
        string filename,
        string contentType)
    {
        var timestamp = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds();
        var s3Key = $"uploads/{timestamp}-{filename}";

        // Generate pre-signed URL (requires AWS credentials and signing logic)
        var uploadUrl = GeneratePresignedUrl(s3Key, contentType);

        return new UploadInfo { UploadUrl = uploadUrl, S3Key = s3Key };
    }

    // Client uploads directly to S3 using the pre-signed URL, then calls:
    [SpacetimeDB.Reducer]
    public static void ConfirmUpload(ReducerContext ctx, string filename, string s3Key)
    {
        ctx.Db.Document.Insert(new Document
        {
            Id = 0,
            OwnerId = ctx.Sender,
            Filename = filename,
            S3Key = s3Key,
            UploadedAt = ctx.Timestamp,
        });
    }

    private static string GeneratePresignedUrl(string s3Key, string contentType)
    {
        // Implement AWS S3 pre-signed URL generation
        throw new NotImplementedException();
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
#[derive(SpacetimeType)]
pub struct UploadInfo {
    upload_url: String,
    s3_key: String,
}

// Procedure returns a pre-signed URL for client-side upload
#[spacetimedb::procedure]
pub fn get_upload_url(
    _ctx: &mut ProcedureContext,
    filename: String,
    _content_type: String,
) -> Result<UploadInfo, String> {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    let s3_key = format!("uploads/{}-{}", timestamp, filename);

    // Generate pre-signed URL (requires AWS credentials and signing logic)
    let upload_url = generate_presigned_url(&s3_key)?;

    Ok(UploadInfo { upload_url, s3_key })
}

// Client uploads directly to S3 using the pre-signed URL, then calls:
#[spacetimedb::reducer]
pub fn confirm_upload(ctx: &ReducerContext, filename: String, s3_key: String) {
    ctx.db.document().insert(Document {
        id: 0,
        owner_id: ctx.sender,
        filename,
        s3_key,
        uploaded_at: ctx.timestamp,
    });
}
```

</TabItem>
</Tabs>

The pre-signed URL approach is preferred for large files because:
- **No size limits**: Files don't pass through SpacetimeDB
- **Better performance**: Direct client-to-S3 transfer
- **Reduced load**: SpacetimeDB only handles metadata

## Hybrid Approach: Thumbnails and Originals

For images, store small thumbnails inline for fast access while keeping originals in external storage:

<Tabs groupId="server-language" queryString>
<TabItem value="typescript" label="TypeScript">

```typescript
import { table, t, schema } from 'spacetimedb/server';

const image = table(
  { name: 'image', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    ownerId: t.identity().index('btree'),
    thumbnail: t.array(t.u8()),      // Small preview stored inline
    originalUrl: t.string(),          // Large original in external storage
    width: t.u32(),
    height: t.u32(),
    uploadedAt: t.timestamp(),
  }
);
```

</TabItem>
<TabItem value="csharp" label="C#">

```csharp
using SpacetimeDB;

public partial class Module
{
    [SpacetimeDB.Table(Accessor = "Image", Public = true)]
    public partial struct Image
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;
        [SpacetimeDB.Index.BTree]
        public Identity OwnerId;
        public List<byte> Thumbnail;      // Small preview stored inline
        public string OriginalUrl;        // Large original in external storage
        public uint Width;
        public uint Height;
        public Timestamp UploadedAt;
    }
}
```

</TabItem>
<TabItem value="rust" label="Rust">

```rust
use spacetimedb::{Identity, Timestamp};

#[spacetimedb::table(accessor = image, public)]
pub struct Image {
    #[primary_key]
    #[auto_inc]
    id: u64,
    #[index(btree)]
    owner_id: Identity,
    thumbnail: Vec<u8>,      // Small preview stored inline
    original_url: String,    // Large original in external storage
    width: u32,
    height: u32,
    uploaded_at: Timestamp,
}
```

</TabItem>
<TabItem value="cpp" label="C++">

```cpp
struct Image {
    uint64_t id;
    Identity owner_id;
    std::vector<uint8_t> thumbnail;  // Small preview stored inline
    std::string original_url;        // Large original in external storage
    uint32_t width;
    uint32_t height;
    Timestamp uploaded_at;
};
SPACETIMEDB_STRUCT(Image, id, owner_id, thumbnail, original_url, width, height, uploaded_at)
SPACETIMEDB_TABLE(Image, image, Public)
FIELD_PrimaryKeyAutoInc(image, id)
FIELD_Index(image, owner_id)
```

</TabItem>
</Tabs>

This approach provides:
- **Fast thumbnail access** through subscriptions (no extra network requests)
- **Efficient storage** for large originals (external storage optimized for blobs)
- **Real-time updates** for metadata changes through SpacetimeDB subscriptions

## Choosing a Strategy

| Scenario | Recommended Approach |
|----------|---------------------|
| User avatars (< 10MB) | Inline storage |
| Chat attachments (< 50MB) | Inline storage |
| Document uploads (< 100MB) | Inline storage |
| Large files (> 100MB) | External storage with reference |
| Video files | External storage with CDN |
| Images with previews | Hybrid (inline thumbnail + external original) |

SpacetimeDB storage costs approximately $1/GB compared to cheaper blob storage options like AWS S3. For large files that don't need atomic updates with other data, external storage may be more economical.

The right choice depends on your file sizes, access patterns, and whether the data needs to participate in real-time subscriptions.
