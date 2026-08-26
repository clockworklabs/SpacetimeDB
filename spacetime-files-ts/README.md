# @spacetimedb/files

File storage primitives for SpacetimeDB modules: upload, list, delete, and serve
byte blobs with per-file visibility, SHA-256 ETags, and an HTTP handler factory
that streams cached responses through the module's route.

---

## Install

```bash
npm install @spacetimedb/files spacetimedb@^2.8.3
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

Bytes live in the module's `file` table as transactional application state.

## Usage

### Integrate into an application

For a new application, mount the submodule first. The host must derive an owner
from its own identity or session model and expose narrow wrappers around the
file helpers. Keep the file table private.

```ts
import { schema, t } from 'spacetimedb/server';
import * as files from '@spacetimedb/files/submodule';

const spacetimedb = schema({ files });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  files.installFiles(ctx.as.files);
});

export const upload_file = spacetimedb.procedure(
  files.uploadFileParams,
  t.u64(),
  (ctx, args) =>
    files.uploadFileImpl(ctx.as.files, args, ctx.sender.toHexString())
);
```

The host module owns `init`, derives owners from its auth model, and wraps the
helper procedures and HTTP handler with `ctx.as.files`.
See the
[Vault host module](./example/spacetimedb/)
for upload wrappers, scoped metadata views, and an HTTP download route.

### Standalone table builders

Use the lower-level row and implementation exports only when the host needs to
own the file tables directly:

```ts
import { fileRow } from '@spacetimedb/files/rows';
```

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

The private `file_blob` table stores `{ fileId, bytes }` separately. Metadata
lookups, `HEAD`, and conditional `304` responses therefore avoid reading or
copying the blob. `GET` and authenticated byte procedures load it after access
checks pass.

The mounted table is private. Host views should return `fileSummary` rows so
subscriptions carry safe metadata fields.

The package also exports `fileSummary`, a safe metadata shape that omits
`ownerUserId`, `ownerPathKey`, and blob bytes, for use in procedure and view
return types.

After generating bindings, upload through the host wrapper and subscribe to a
host view that returns file summaries:

```ts
import { tables } from './module_bindings';

const fileId = await conn.procedures.uploadFile({
  path: '/avatars/me.png',
  mimeType: 'image/png',
  bytes: pngBytes,
  visibility: 'owner',
});

conn.subscriptionBuilder().subscribe([tables.myFileSummaries]);
```

## API

Each `*Impl` takes `(ctx, args, owner)` so the submodule stays identity-scheme-agnostic. Wrap them with thin reducers in your app module that derive `owner` however you want (caller `Identity`, a session lookup through a mounted auth namespace, etc).

Package entrypoints:

- `@spacetimedb/files/submodule` supplies the mountable namespace and all
  host integration helpers.
- `@spacetimedb/files` exports the lower-level rows, validation,
  procedures, constants, and HTTP handler.
- `@spacetimedb/files/procedures` exports operation parameters, return
  types, and implementations.
- `@spacetimedb/files/handlers` exports public-file HTTP serving.
- `@spacetimedb/files/rows` exports table row builders.
- `@spacetimedb/files/constants` is safe to import in browser code.

Validation exports include `validateFileOwner`, `validateFilePath`,
`validateFilePrefix`, `validateMimeType`, `safeMimeType`, `ownerPathKey`, and
`FileValidationError`.

### `uploadFile`

```ts
import {
  uploadFileParams,
  uploadFileImpl,
} from '@spacetimedb/files/procedures';
```

- Args: `path`, `mimeType`, `bytes` (`u8[]`), `visibility`.
- Returns: `bigint` (the file `id`).
- Upserts by the owner/path pair. Different owners may use the same path.
- Requires an absolute canonical path such as `/images/avatar.png`.
- Enforces `bytes.length <= FILE_BYTES_MAX` (4 MB) and `path.length <= 1024`.
- Accepts a media type such as `image/png` or `image/svg+xml`. Parameters and
  control characters are rejected.
- Computes the authoritative `sha256Hex` ETag server-side.

### `deleteFile`

- Args: `path`.
- Owner-gated through the owner/path key.
- Returns nothing when the caller has no file at that path.

### `listFiles`

```ts
import {
  listFilesParams,
  listFilesReturn,
  listFilesImpl,
} from '@spacetimedb/files/procedures';
```

- Args: `prefix`, optional `cursor`, and optional `limit` from 1 to 200.
- Returns: `{ files, nextCursor }`, ordered by `path`. Pass `nextCursor` into
  the next call until it is absent. `bytes` is omitted.
- Scopes to the caller's own files.

### `setFileVisibility`

- Args: `path`, `visibility`.
- Owner-gated.

### `readFileBytes`

```ts
import {
  readFileBytesParams,
  readFileBytesReturn,
  readFileBytesImpl,
} from '@spacetimedb/files/procedures';
```

- Args: `path`. Returns: `{ bytes: u8[], mimeType: string }`.
- Owner-gated. Throws `files.not_found` / `files.not_owner`.
- **Private files use an authenticated procedure.** SpacetimeDB HTTP route
  handlers see the _module's_ identity, so `makeFileServeImpl` serves public
  files. Procedures receive the authenticated sender. Wrap `readFileBytesImpl`
  in a procedure for private previews and downloads, and use HTTP for cacheable
  public files.

```ts
export const read_file_bytes = spacetimedb.procedure(
  readFileBytesParams,
  readFileBytesReturn,
  (ctx, args) => readFileBytesImpl(ctx, args, ctx.sender.toHexString())
);
```

## HTTP serve handler

```ts
import { makeFileServeImpl } from '@spacetimedb/files/handlers';
```

Wire a handler into your module's HTTP routes:

```ts
const serveFile = makeFileServeImpl({
  getOwner: _ctx => undefined,
});
```

Mount it under a route like `/files/*` from your module. The handler:

- Accepts `GET` and `HEAD` only; everything else 405s.
- Reads the stable file ID from `?id=<fileId>`.
- Returns 404 for an unknown file and 403 when an owner-only file has a different owner.
- Sends `etag: "<sha256Hex>"` and honors `If-None-Match` with 304.
- Sets `cache-control: public, max-age=300, must-revalidate` for public files; `private, max-age=60, must-revalidate` for owner files.
- `HEAD` returns headers only; `GET` returns the full body.

`getOwner` is the host's authentication hook. Return the authenticated owner
value when private HTTP reads are supported. Returning `undefined` limits the
route to public files.

## Constants

| Constant                 | Value       |
| ------------------------ | ----------- |
| `FILE_BYTES_MAX`         | `4_000_000` |
| `FILE_PATH_MAX`          | `1024`      |
| `FILE_MIME_TYPE_MAX`     | `127`       |
| `FILE_LIST_PAGE_MAX`     | `200`       |
| `FILE_VISIBILITY_OWNER`  | `'owner'`   |
| `FILE_VISIBILITY_PUBLIC` | `'public'`  |

The limits also ship from the browser-safe `./constants` subpath. It has no
server imports, so clients can pre-validate uploads with the same values.

## Errors

All thrown as `SenderError` with stable codes:

- `files.invalid_path` - non-canonical, unsafe, or longer than 1024
- `files.invalid_prefix` / `files.invalid_cursor` - invalid listing position
- `files.invalid_page_size` - listing limit outside 1 to 200
- `files.invalid_mime_type` - invalid or unsafe HTTP media type
- `files.invalid_visibility:<value>` - not in `{owner, public}`
- `files.too_large:<actual>/<max>` - body exceeds `FILE_BYTES_MAX`
- `files.not_found:<path>` - `setFileVisibility` or `readFileBytes` on a missing row

## Testing

```bash
pnpm test
pnpm run typecheck
```

Build the
[example host module](./example/spacetimedb/)
to verify the
mounted submodule and generated bindings together.

## License

[BUSL-1.1](./LICENSE.txt) - same as SpacetimeDB.
