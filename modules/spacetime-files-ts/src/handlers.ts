import { SyncResponse, type Infer, type Request } from 'spacetimedb/server';
import { FILE_VISIBILITY_PUBLIC } from './constants.js';
import { responseHeaders } from './headers.js';
import { queryParam } from './query.js';
import type { file, FilesHandlerCtx } from './submodule/schema.js';
import { safeMimeType } from './validation.js';

type FileRow = Infer<typeof file.rowType>;

export type FileMetadata = ReturnType<typeof snapshotMetadata>;

function snapshotMetadata(file: FileRow) {
  return {
    id: file.id,
    path: file.path,
    ownerUserId: file.ownerUserId,
    mimeType: safeMimeType(file.mimeType),
    size: file.size,
    sha256Hex: file.sha256Hex,
    visibility: file.visibility,
    createdAt: file.createdAt,
    updatedAt: file.updatedAt,
  };
}

/**
 * Serves `?id=<fileId>` for GET and HEAD. Public files are always served;
 * others only when `canAccess` returns true. Call with `ctx.as.files`.
 */
export function serveFile(
  ctx: FilesHandlerCtx,
  req: Request,
  canAccess: (file: FileMetadata) => boolean = () => false
): SyncResponse {
  const method = req.method.toUpperCase();
  if (method !== 'GET' && method !== 'HEAD') {
    return new SyncResponse('method not allowed', { status: 405 });
  }

  const rawId = queryParam(String(req.uri), 'id');
  if (!rawId) return new SyncResponse('missing id', { status: 400 });
  let id: bigint;
  try {
    id = BigInt(rawId);
    if (id <= 0n) return new SyncResponse('bad id', { status: 400 });
  } catch {
    return new SyncResponse('bad id', { status: 400 });
  }

  const allowed = (file: FileMetadata) =>
    file.visibility === FILE_VISIBILITY_PUBLIC || canAccess(file);

  const metadata = ctx.withTx(tx => {
    const row = tx.db.file.id.find(id);
    return row ? snapshotMetadata(row) : undefined;
  });
  if (!metadata) return new SyncResponse('not found', { status: 404 });
  if (!allowed(metadata)) return new SyncResponse('forbidden', { status: 403 });

  const headers = responseHeaders(metadata);
  if (req.headers.get('if-none-match') === headers.etag) {
    return new SyncResponse('', {
      status: 304,
      headers: { etag: headers.etag },
    });
  }
  if (method === 'HEAD') return new SyncResponse('', { status: 200, headers });

  // Load bytes only for a GET that needs a body. Recheck access against the
  // same snapshot so a visibility change cannot race the metadata lookup.
  const file = ctx.withTx(tx => {
    const row = tx.db.file.id.find(id);
    const blob = row && tx.db.fileBlob.fileId.find(id);
    return blob ? { ...snapshotMetadata(row), bytes: blob.bytes } : undefined;
  });
  if (!file) return new SyncResponse('not found', { status: 404 });
  if (!allowed(file)) return new SyncResponse('forbidden', { status: 403 });
  const finalHeaders = responseHeaders(file);
  if (req.headers.get('if-none-match') === finalHeaders.etag) {
    return new SyncResponse('', {
      status: 304,
      headers: { etag: finalHeaders.etag },
    });
  }
  return new SyncResponse(new Uint8Array(file.bytes), {
    status: 200,
    headers: finalHeaders,
  });
}
