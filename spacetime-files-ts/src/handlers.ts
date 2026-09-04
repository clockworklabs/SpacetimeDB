import { SyncResponse, type Infer, type Request } from 'spacetimedb/server';
import { fileBlobRow, fileRow, FILE_VISIBILITY_PUBLIC } from './rows.ts';
import { queryParam } from './query.ts';
import { safeMimeType } from './validation.ts';

type FileRow = Infer<typeof fileRow>;
type FileBlobRow = Infer<typeof fileBlobRow>;

interface FileTableLike {
  id: { find(id: bigint): FileRow | null | undefined };
}

interface FileBlobTableLike {
  fileId: { find(id: bigint): FileBlobRow | null | undefined };
}

interface FileDbLike {
  file?: FileTableLike;
  fileBlob?: FileBlobTableLike;
  files?: {
    file?: FileTableLike;
    fileBlob?: FileBlobTableLike;
  };
}

interface FileTransactionLike {
  db: FileDbLike;
}

export interface FileHandlerContext {
  identity?: { toHexString(): string };
  withTx<T, Tx extends FileTransactionLike = FileTransactionLike>(
    body: (tx: Tx) => T
  ): T;
}

type FileMetadata = ReturnType<typeof snapshotMetadata>;

export interface FileServeOptions {
  getOwner: (ctx: FileHandlerContext, req: Request) => string | undefined;
  canAccess?: (
    ctx: FileHandlerContext,
    req: Request,
    file: FileMetadata,
    owner: string | undefined
  ) => boolean;
}

function getFileTable(db: FileDbLike): FileTableLike | undefined {
  return db.file ?? db.files?.file;
}

function getFileBlobTable(db: FileDbLike): FileBlobTableLike | undefined {
  return db.fileBlob ?? db.files?.fileBlob;
}

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

function snapshotWithBytes(file: FileRow, bytes: number[]) {
  return {
    ...snapshotMetadata(file),
    bytes: new Uint8Array(bytes),
  };
}

function responseHeaders(
  file: ReturnType<typeof snapshotMetadata>
): Record<string, string> {
  return {
    'content-type': file.mimeType,
    'content-length': String(file.size),
    etag: `"${file.sha256Hex}"`,
    'cache-control':
      file.visibility === FILE_VISIBILITY_PUBLIC
        ? 'public, max-age=300, must-revalidate'
        : 'private, max-age=60, must-revalidate',
  };
}

export function createFileHttpHandler(opts: FileServeOptions) {
  return (rawCtx: unknown, req: Request): SyncResponse => {
    const ctx = rawCtx as FileHandlerContext;
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

    const metadata = ctx.withTx(tx => {
      const row = getFileTable(tx.db)?.id.find(id);
      return row ? snapshotMetadata(row) : undefined;
    });
    if (!metadata) return new SyncResponse('not found', { status: 404 });

    const owner = opts.getOwner(ctx, req);
    const canAccess = (file: FileMetadata) =>
      file.visibility === FILE_VISIBILITY_PUBLIC ||
      (opts.canAccess
        ? opts.canAccess(ctx, req, file, owner)
        : Boolean(owner && file.ownerUserId === owner));
    if (!canAccess(metadata))
      return new SyncResponse('forbidden', { status: 403 });

    const headers = responseHeaders(metadata);
    if (req.headers.get('if-none-match') === headers.etag) {
      return new SyncResponse('', {
        status: 304,
        headers: { etag: headers.etag },
      });
    }
    if (method === 'HEAD') {
      return new SyncResponse('', { status: 200, headers });
    }

    // Load bytes only for a GET that needs a body. Recheck access against the
    // same snapshot so a visibility change cannot race the metadata lookup.
    const file = ctx.withTx(tx => {
      const row = getFileTable(tx.db)?.id.find(id);
      if (!row) return undefined;
      const blob = getFileBlobTable(tx.db)?.fileId.find(id);
      return blob ? snapshotWithBytes(row, blob.bytes) : undefined;
    });
    if (!file) return new SyncResponse('not found', { status: 404 });
    if (!canAccess(file)) return new SyncResponse('forbidden', { status: 403 });
    const finalHeaders = responseHeaders(file);
    if (req.headers.get('if-none-match') === finalHeaders.etag) {
      return new SyncResponse('', {
        status: 304,
        headers: { etag: finalHeaders.etag },
      });
    }
    return new SyncResponse(file.bytes, { status: 200, headers: finalHeaders });
  };
}
