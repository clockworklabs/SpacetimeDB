---
title: File Storage
slug: /tables/file-storage
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';


SpacetimeDB can store binary data directly in table columns, making it suitable for files, images, and other blobs that need to participate in transactions and subscriptions.

## Storing Binary Data Inline

Store binary data using `Vec<u8>` (Rust), `List<byte>` (C#), or `t.array(t.u8())` (TypeScript). This approach keeps data within the database, ensuring it participates in transactions and real-time updates.

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

const spacetimedb = schema(userAvatar);

spacetimedb.reducer('upload_avatar', {
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
    [SpacetimeDB.Table(Name = "UserAvatar", Public = true)]
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

#[spacetimedb::table(name = user_avatar, public)]
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
</Tabs>

### When to Use Inline Storage

Inline storage works well for:

- **Small to medium files** (up to a few megabytes)
- **Data that changes with other row fields** (e.g., user profile with avatar)
- **Data requiring transactional consistency** (file updates atomic with metadata)
- **Data clients need through subscriptions** (real-time avatar updates)

### Size Considerations

Each row has practical size limits. Very large binary data affects:

- **Memory usage**: Rows are held in memory during reducer execution
- **Network bandwidth**: Large rows increase subscription traffic
- **Transaction size**: Large rows slow down transaction commits

For files larger than a few megabytes, consider external storage.

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

const spacetimedb = schema(document);

// Called after uploading file to external storage
spacetimedb.reducer('register_document', {
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
    [SpacetimeDB.Table(Name = "Document", Public = true)]
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

#[spacetimedb::table(name = document, public)]
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
    [SpacetimeDB.Table(Name = "Image", Public = true)]
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

#[spacetimedb::table(name = image, public)]
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
</Tabs>

This approach provides:
- **Fast thumbnail access** through subscriptions (no extra network requests)
- **Efficient storage** for large originals (external storage optimized for blobs)
- **Real-time updates** for metadata changes through SpacetimeDB subscriptions

## Choosing a Strategy

| Scenario | Recommended Approach |
|----------|---------------------|
| User avatars (< 100KB) | Inline storage |
| Chat attachments (< 1MB) | Inline storage |
| Document uploads (> 1MB) | External storage with reference |
| Video files | External storage with CDN |
| Images with previews | Hybrid (inline thumbnail + external original) |

The right choice depends on your file sizes, access patterns, and whether the data needs to participate in real-time subscriptions.
