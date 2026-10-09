# @spacetimedb/files

Store and share files in your SpacetimeDB application, such as avatars and
message attachments. Users can upload, list, rename, and delete their files.
Keep files private or make them available through an HTTP download route.

File contents are stored in your database. Each file can be up to 4 MB, with a
default total limit of 100 MB per owner.

## Install

```bash
npm install @spacetimedb/files spacetimedb
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

## Integrate into an application

Add Files to your module, then expose operations for uploading and reading a
file. This example uses the caller's SpacetimeDB identity as the owner. If your
application has user accounts, use the authenticated user ID instead.

The file helpers run in your transaction: pass `ctx.as.files` from a reducer,
or `tx.as.files` inside a procedure's `withTx`.

```ts
import { schema } from 'spacetimedb/server';
import * as files from '@spacetimedb/files/submodule';

const spacetimedb = schema({ files });
export default spacetimedb;

export const uploadFile = spacetimedb.reducer(
  files.uploadFileParams,
  (ctx, args) => {
    files.uploadFile(ctx.as.files, args, ctx.sender.toHexString());
  }
);

export const readFileBytes = spacetimedb.procedure(
  files.readFileBytesParams,
  files.readFileBytesReturn,
  (ctx, args) =>
    ctx.withTx(tx =>
      files.readFileBytes(tx.as.files, args, ctx.sender.toHexString())
    )
);
```

See the [Vault host module](./example/spacetimedb/) for folder handling,
scoped metadata views, and an HTTP download route.

Host views should return `fileSummary` rows, which omit `ownerUserId`,
`ownerPathKey`, and blob bytes, so subscriptions carry safe metadata only:

```ts
import { tables } from './module_bindings';

await conn.reducers.uploadFile({
  path: '/avatars/me.png',
  mimeType: 'image/png',
  bytes: pngBytes,
  visibility: 'owner',
});

conn.subscriptionBuilder().subscribe([tables.myFileSummaries]);
```

### Standalone table builders

`@spacetimedb/files` exports `fileRow` and `fileBlobRow` for hosts that own a
file-like table. The helpers below operate on the submodule tables only.

| Field                     | Type              | Notes                                                          |
| ------------------------- | ----------------- | -------------------------------------------------------------- |
| `id`                      | `u64` PK auto-inc |                                                                |
| `ownerPathKey`            | `string` unique   | Collision-safe internal owner/path key                         |
| `path`                    | `string` indexed  | Canonical caller-supplied path, up to 1024 chars               |
| `ownerUserId`             | `string` indexed  | Opaque identity, application user ID, or host-defined actor ID |
| `mimeType`                | `string`          |                                                                |
| `size`                    | `u64`             |                                                                |
| `sha256Hex`               | `string`          | Lowercase hex of `SHA-256(bytes)`; used as strong ETag         |
| `visibility`              | `string` indexed  | `FILE_VISIBILITY_OWNER` or `FILE_VISIBILITY_PUBLIC`            |
| `createdAt` / `updatedAt` | `timestamp`       |                                                                |

`file_blob` stores `{ fileId, bytes }` separately, so metadata lookups, `HEAD`,
and conditional `304` responses avoid reading the blob.

## API

Package entrypoints:

- `@spacetimedb/files/submodule`: the submodule namespace, helpers, and
  `serveFile`.
- `@spacetimedb/files`: row builders, `fileSummary`, constants, and `errors`.
- `@spacetimedb/files/constants`: constants and `errors`, safe to import in
  browser code.

Each helper takes `(ctx, args, owner)`. Derive `owner` from the authenticated
caller.

### `uploadFile(ctx, args, owner, opts?)`

- Args: `path`, `mimeType`, `bytes` (`u8[]`), `visibility`.
- Returns: `bigint` (the file `id`).
- Upserts by the owner/path pair. Different owners may use the same path.
- Requires an absolute canonical path such as `/images/avatar.png`.
- Enforces `bytes.length <= FILE_BYTES_MAX` (4 MB) and `path.length <= 1024`.
- Enforces a per-owner total of `opts.maxOwnerBytes`, default
  `FILE_OWNER_BYTES_MAX` (100 MB).
- Accepts a media type such as `image/png` or `image/svg+xml`. Parameters and
  control characters are rejected.
- Computes the authoritative `sha256Hex` ETag server-side.

### `renameFile(ctx, args, owner)`

- Args: `oldPath`, `newPath`.
- Throws `files.not_found` for a missing file and `files.path_taken` when the
  owner already has a file at `newPath`.

### `deleteFile(ctx, args, owner)`

- Args: `path`.
- Does nothing when the owner has no file at that path.

### `listFiles(ctx, args, owner)`

- Args: `prefix`, optional `cursor`, and optional `limit` from 1 to 200.
- Returns: `{ files, nextCursor }`, ordered by `path`. Pass `nextCursor` into
  the next call until it is absent. `bytes` is omitted.

### `setFileVisibility(ctx, args, owner)`

- Args: `path`, `visibility`.

### `readFileBytes(ctx, args, owner)`

- Args: `path`. Returns: `{ bytes: u8[], mimeType: string }`.
- HTTP handlers have no caller identity, so private files are read through a
  procedure that knows the sender. Use HTTP for cacheable public files.

## HTTP serve handler

```ts
export const fileServe = spacetimedb.httpHandler((ctx, req) =>
  files.serveFile(ctx.as.files, req)
);

export const router = spacetimedb.httpRouter(
  new Router().get('/files', fileServe).head('/files', fileServe)
);
```

`serveFile(ctx, req, canAccess?)`:

- Accepts `GET` and `HEAD` only; everything else 405s.
- Reads the stable file ID from `?id=<fileId>`.
- Serves public files. Other files are served only when `canAccess(file)`
  returns true; without it they return 403. A host that authenticates HTTP
  requests (for example with `@spacetimedb/auth`'s `requestUserId`) decides
  access there.
- Returns 404 for an unknown file.
- Sends `etag: "<sha256Hex>"` and honors `If-None-Match` with 304.
- Sets `cache-control: public, max-age=300, must-revalidate` for public files
  and `private, max-age=60, must-revalidate` for others.
- Sends `x-content-type-options: nosniff` and
  `content-security-policy: sandbox`. PNG, JPEG, GIF, WebP, and AVIF images
  render inline; every other type is sent with `content-disposition:
attachment`, so uploaded HTML or SVG never runs on the serving origin.

## Constants

| Constant                 | Value         |
| ------------------------ | ------------- |
| `FILE_BYTES_MAX`         | `4_000_000`   |
| `FILE_OWNER_BYTES_MAX`   | `100_000_000` |
| `FILE_PATH_MAX`          | `1024`        |
| `FILE_MIME_TYPE_MAX`     | `127`         |
| `FILE_LIST_PAGE_MAX`     | `200`         |
| `FILE_VISIBILITY_OWNER`  | `'owner'`     |
| `FILE_VISIBILITY_PUBLIC` | `'public'`    |

## Errors

Thrown as `SenderError` with the codes in `errors`:

- `files.invalid_owner` - empty, longer than 512, or containing control characters
- `files.invalid_path` - non-canonical, unsafe, or longer than 1024
- `files.invalid_prefix` / `files.invalid_cursor` - invalid listing position
- `files.invalid_page_size` - listing limit outside 1 to 200
- `files.invalid_mime_type` - invalid or unsafe HTTP media type
- `files.invalid_visibility` - not `owner` or `public`
- `files.too_large:<actual>/<max>` - body exceeds `FILE_BYTES_MAX`
- `files.quota_exceeded` - the owner's total would exceed the quota
- `files.not_found` - no file at that path for this owner
- `files.path_taken` - `renameFile` target already exists

## Testing

```bash
pnpm test
pnpm run typecheck
```

## License

Apache-2.0.
