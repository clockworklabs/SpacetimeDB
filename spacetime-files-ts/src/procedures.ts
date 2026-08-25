// Owner passed explicitly so the submodule is identity-scheme-agnostic.
import type { Timestamp } from 'spacetimedb';
import {
  Range,
  t,
  SenderError,
  type InferTypeOfParams,
} from 'spacetimedb/server';
import {
  fileListPage,
  FILE_VISIBILITY_OWNER,
  FILE_VISIBILITY_PUBLIC,
} from './rows.ts';
import { FILE_BYTES_MAX, FILE_LIST_PAGE_MAX } from './constants.ts';
import { fileSha256Hex } from './hash.ts';
import {
  FileValidationError,
  ownerPathKey,
  validateFileOwner,
  validateFilePath,
  validateFilePrefix,
  validateMimeType,
} from './validation.ts';
import type { TransactionModuleCtx } from './submodule/schema.ts';

type FileTable = TransactionModuleCtx['db']['file'];
type FileBlobTable = TransactionModuleCtx['db']['fileBlob'];

interface FileDbLike {
  file?: FileTable;
  fileBlob?: FileBlobTable;
  files?: {
    file?: FileTable;
    fileBlob?: FileBlobTable;
  };
}

interface FileTransactionLike {
  db: FileDbLike;
}

interface FileProcedureContext {
  timestamp: Timestamp;
  withTx<T>(body: (tx: FileTransactionLike) => T): T;
}

// Re-exported for compatibility; canonical home is ./constants.ts (browser-safe).
export { FILE_BYTES_MAX, FILE_PATH_MAX } from './constants.ts';

// Lowercase hex SHA-256, for consumers that write their own insert path.
export { fileSha256Hex } from './hash.ts';

const VALID_VISIBILITIES = new Set([
  FILE_VISIBILITY_OWNER,
  FILE_VISIBILITY_PUBLIC,
]);

// Direct `file` table or mounted-submodule layout, as in handlers.ts.
function fileTable(db: FileDbLike): FileTable {
  const table = db.file ?? db.files?.file;
  if (!table) throw new Error('files.file table is unavailable');
  return table;
}

function fileBlobTable(db: FileDbLike): FileBlobTable {
  const table = db.fileBlob ?? db.files?.fileBlob;
  if (!table) throw new Error('files.fileBlob table is unavailable');
  return table;
}

function validated<T>(fn: () => T): T {
  try {
    return fn();
  } catch (error) {
    if (error instanceof FileValidationError)
      throw new SenderError(error.message);
    throw error;
  }
}

function prefixUpperBound(prefix: string): string | undefined {
  if (prefix.length === 0) return undefined;
  const units = Array.from(prefix);
  for (let i = units.length - 1; i >= 0; i--) {
    const code = units[i]!.codePointAt(0)!;
    if (code < 0x10ffff) {
      units[i] = String.fromCodePoint(code + 1);
      return units.slice(0, i + 1).join('');
    }
  }
  return undefined;
}

export const uploadFileParams = {
  path: t.string(),
  mimeType: t.string(),
  bytes: t.array(t.u8()),
  visibility: t.string(),
};

export function uploadFileImpl(
  rawCtx: unknown,
  args: InferTypeOfParams<typeof uploadFileParams>,
  owner: string
): bigint {
  const ctx = rawCtx as FileProcedureContext;
  owner = validated(() => validateFileOwner(owner));
  const path = validated(() => validateFilePath(args.path));
  const mimeType = validated(() => validateMimeType(args.mimeType));
  if (args.bytes.length > FILE_BYTES_MAX) {
    throw new SenderError(
      `files.too_large:${args.bytes.length}/${FILE_BYTES_MAX}`
    );
  }
  if (!VALID_VISIBILITIES.has(args.visibility)) {
    throw new SenderError(`files.invalid_visibility:${args.visibility}`);
  }
  const sha256Hex = fileSha256Hex(args.bytes);
  const key = ownerPathKey(owner, path);
  return ctx.withTx(tx => {
    const files = fileTable(tx.db);
    const blobs = fileBlobTable(tx.db);
    const existing = files.ownerPathKey.find(key);
    if (existing) {
      files.id.update({
        ...existing,
        mimeType,
        size: BigInt(args.bytes.length),
        sha256Hex,
        visibility: args.visibility,
        updatedAt: ctx.timestamp,
      });
      blobs.fileId.update({ fileId: existing.id, bytes: args.bytes });
      return existing.id;
    }
    const row = files.insert({
      id: 0n,
      ownerPathKey: key,
      path,
      ownerUserId: owner,
      mimeType,
      size: BigInt(args.bytes.length),
      sha256Hex,
      visibility: args.visibility,
      createdAt: ctx.timestamp,
      updatedAt: ctx.timestamp,
    });
    blobs.insert({ fileId: row.id, bytes: args.bytes });
    return row.id;
  });
}

export const deleteFileParams = {
  path: t.string(),
};

export function deleteFileImpl(
  rawCtx: unknown,
  args: InferTypeOfParams<typeof deleteFileParams>,
  owner: string
): void {
  const ctx = rawCtx as FileProcedureContext;
  owner = validated(() => validateFileOwner(owner));
  const path = validated(() => validateFilePath(args.path));
  ctx.withTx(tx => {
    const files = fileTable(tx.db);
    const blobs = fileBlobTable(tx.db);
    const row = files.ownerPathKey.find(ownerPathKey(owner, path));
    if (!row) return;
    const blob = blobs.fileId.find(row.id);
    if (blob) blobs.delete(blob);
    files.delete(row);
  });
}

export const listFilesParams = {
  prefix: t.string(),
  cursor: t.option(t.string()),
  limit: t.option(t.u32()),
};

export const listFilesReturn = fileListPage;

// Caller's own files; bytes omitted (fetch via HTTP handler).
export function listFilesImpl(
  rawCtx: unknown,
  args: InferTypeOfParams<typeof listFilesParams>,
  owner: string
) {
  const ctx = rawCtx as FileProcedureContext;
  owner = validated(() => validateFileOwner(owner));
  const prefix = validated(() => validateFilePrefix(args.prefix));
  const rawCursor = args.cursor;
  const cursor =
    rawCursor === undefined
      ? undefined
      : validated(() => validateFilePath(rawCursor));
  if (cursor !== undefined && !cursor.startsWith(prefix)) {
    throw new SenderError('files.invalid_cursor');
  }
  const limit = args.limit ?? 100;
  if (!Number.isInteger(limit) || limit < 1 || limit > FILE_LIST_PAGE_MAX) {
    throw new SenderError('files.invalid_page_size');
  }
  return ctx.withTx(tx => {
    const out: Array<{
      id: bigint;
      path: string;
      mimeType: string;
      size: bigint;
      sha256Hex: string;
      visibility: string;
      updatedAt: Timestamp;
    }> = [];
    const from =
      cursor === undefined
        ? prefix === ''
          ? undefined
          : { tag: 'included' as const, value: prefix }
        : { tag: 'excluded' as const, value: cursor };
    const upper = prefixUpperBound(prefix);
    const to =
      upper === undefined
        ? undefined
        : { tag: 'excluded' as const, value: upper };
    for (const row of fileTable(tx.db).ownerPath.filter([
      owner,
      new Range(from, to),
    ])) {
      out.push({
        id: row.id,
        path: row.path,
        mimeType: row.mimeType,
        size: row.size,
        sha256Hex: row.sha256Hex,
        visibility: row.visibility,
        updatedAt: row.updatedAt,
      });
      if (out.length > limit) break;
    }
    const hasMore = out.length > limit;
    if (hasMore) out.pop();
    return {
      files: out,
      nextCursor: hasMore ? out.at(-1)?.path : undefined,
    };
  });
}

export const readFileBytesParams = {
  path: t.string(),
};

export const readFileBytesReturn = t.object('FileBytes', {
  bytes: t.array(t.u8()),
  mimeType: t.string(),
});

// Owner-gated byte read. HTTP handlers never see the caller's identity, so
// private files can only be read here, over the authenticated connection.
export function readFileBytesImpl(
  rawCtx: unknown,
  args: InferTypeOfParams<typeof readFileBytesParams>,
  owner: string
): { bytes: number[]; mimeType: string } {
  const ctx = rawCtx as FileProcedureContext;
  owner = validated(() => validateFileOwner(owner));
  const path = validated(() => validateFilePath(args.path));
  return ctx.withTx(tx => {
    const row = fileTable(tx.db).ownerPathKey.find(ownerPathKey(owner, path));
    if (!row) throw new SenderError(`files.not_found:${path}`);
    const blob = fileBlobTable(tx.db).fileId.find(row.id);
    if (!blob) throw new SenderError(`files.not_found:${path}`);
    return { bytes: blob.bytes, mimeType: row.mimeType };
  });
}

export const setFileVisibilityParams = {
  path: t.string(),
  visibility: t.string(),
};

export function setFileVisibilityImpl(
  rawCtx: unknown,
  args: InferTypeOfParams<typeof setFileVisibilityParams>,
  owner: string
): void {
  const ctx = rawCtx as FileProcedureContext;
  owner = validated(() => validateFileOwner(owner));
  const path = validated(() => validateFilePath(args.path));
  if (!VALID_VISIBILITIES.has(args.visibility)) {
    throw new SenderError(`files.invalid_visibility:${args.visibility}`);
  }
  ctx.withTx(tx => {
    const files = fileTable(tx.db);
    const row = files.ownerPathKey.find(ownerPathKey(owner, path));
    if (!row) throw new SenderError(`files.not_found:${path}`);
    files.id.update({
      ...row,
      visibility: args.visibility,
      updatedAt: ctx.timestamp,
    });
  });
}
