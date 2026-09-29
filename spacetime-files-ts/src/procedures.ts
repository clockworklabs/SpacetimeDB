// Owner passed explicitly so the submodule is identity-scheme-agnostic. Every
// helper runs in the caller's transaction: pass `ctx.as.files` from a host
// reducer, or `tx.as.files` inside a procedure's `withTx`.
import {
  Range,
  t,
  SenderError,
  type InferTypeOfParams,
} from 'spacetimedb/server';
import { fileListPage } from './rows';
import {
  errors,
  FILE_BYTES_MAX,
  FILE_LIST_PAGE_MAX,
  FILE_OWNER_BYTES_MAX,
  FILE_VISIBILITY_OWNER,
  FILE_VISIBILITY_PUBLIC,
} from './constants';
import { fileSha256Hex } from './hash';
import {
  FileValidationError,
  ownerPathKey,
  validateFileOwner,
  validateFilePath,
  validateFilePrefix,
  validateMimeType,
} from './validation';
import type { FilesCtx } from './submodule/schema';

const VALID_VISIBILITIES = new Set<string>([
  FILE_VISIBILITY_OWNER,
  FILE_VISIBILITY_PUBLIC,
]);

function validated<T>(fn: () => T): T {
  try {
    return fn();
  } catch (error) {
    if (error instanceof FileValidationError)
      throw new SenderError(error.message);
    throw error;
  }
}

function validVisibility(visibility: string): string {
  if (!VALID_VISIBILITIES.has(visibility))
    throw new SenderError(errors.invalidVisibility);
  return visibility;
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

function requireFile(ctx: FilesCtx, owner: string, path: string) {
  const row = ctx.db.file.ownerPathKey.find(ownerPathKey(owner, path));
  if (!row) throw new SenderError(errors.notFound);
  return row;
}

export const uploadFileParams = {
  path: t.string(),
  mimeType: t.string(),
  bytes: t.array(t.u8()),
  visibility: t.string(),
};

export interface UploadFileOpts {
  /** Total bytes this owner may store. Default FILE_OWNER_BYTES_MAX. */
  maxOwnerBytes?: number;
}

/** Creates or replaces the owner's file at `path`. Returns the file id. */
export function uploadFile(
  ctx: FilesCtx,
  args: InferTypeOfParams<typeof uploadFileParams>,
  owner: string,
  opts: UploadFileOpts = {}
): bigint {
  owner = validated(() => validateFileOwner(owner));
  const path = validated(() => validateFilePath(args.path));
  const mimeType = validated(() => validateMimeType(args.mimeType));
  const visibility = validVisibility(args.visibility);
  const size = BigInt(args.bytes.length);
  if (args.bytes.length > FILE_BYTES_MAX) {
    throw new SenderError(
      `${errors.tooLarge}:${args.bytes.length}/${FILE_BYTES_MAX}`
    );
  }
  const key = ownerPathKey(owner, path);
  // ponytail: sums the owner's rows on each upload; keep a running total if
  // owners hold many thousands of files.
  let total = size;
  for (const row of ctx.db.file.ownerUserId.filter(owner)) {
    if (row.ownerPathKey !== key) total += row.size;
  }
  if (total > BigInt(opts.maxOwnerBytes ?? FILE_OWNER_BYTES_MAX))
    throw new SenderError(errors.quotaExceeded);

  const sha256Hex = fileSha256Hex(args.bytes);
  const existing = ctx.db.file.ownerPathKey.find(key);
  if (existing) {
    ctx.db.file.id.update({
      ...existing,
      mimeType,
      size,
      sha256Hex,
      visibility,
      updatedAt: ctx.timestamp,
    });
    ctx.db.fileBlob.fileId.update({ fileId: existing.id, bytes: args.bytes });
    return existing.id;
  }
  const row = ctx.db.file.insert({
    id: 0n,
    ownerPathKey: key,
    path,
    ownerUserId: owner,
    mimeType,
    size,
    sha256Hex,
    visibility,
    createdAt: ctx.timestamp,
    updatedAt: ctx.timestamp,
  });
  ctx.db.fileBlob.insert({ fileId: row.id, bytes: args.bytes });
  return row.id;
}

export const renameFileParams = {
  oldPath: t.string(),
  newPath: t.string(),
};

/** Moves the owner's file to a free path. */
export function renameFile(
  ctx: FilesCtx,
  args: InferTypeOfParams<typeof renameFileParams>,
  owner: string
): void {
  owner = validated(() => validateFileOwner(owner));
  const oldPath = validated(() => validateFilePath(args.oldPath));
  const newPath = validated(() => validateFilePath(args.newPath));
  if (oldPath === newPath) return;
  const row = requireFile(ctx, owner, oldPath);
  const key = ownerPathKey(owner, newPath);
  if (ctx.db.file.ownerPathKey.find(key))
    throw new SenderError(errors.pathTaken);
  ctx.db.file.id.update({
    ...row,
    ownerPathKey: key,
    path: newPath,
    updatedAt: ctx.timestamp,
  });
}

export const deleteFileParams = {
  path: t.string(),
};

/** Deletes the owner's file at `path`, if any. */
export function deleteFile(
  ctx: FilesCtx,
  args: InferTypeOfParams<typeof deleteFileParams>,
  owner: string
): void {
  owner = validated(() => validateFileOwner(owner));
  const path = validated(() => validateFilePath(args.path));
  const row = ctx.db.file.ownerPathKey.find(ownerPathKey(owner, path));
  if (!row) return;
  ctx.db.fileBlob.fileId.delete(row.id);
  ctx.db.file.id.delete(row.id);
}

export const listFilesParams = {
  prefix: t.string(),
  cursor: t.option(t.string()),
  limit: t.option(t.u32()),
};

export const listFilesReturn = fileListPage;

/** The owner's files under `prefix`, ordered by path, without bytes. */
export function listFiles(
  ctx: FilesCtx,
  args: InferTypeOfParams<typeof listFilesParams>,
  owner: string
) {
  owner = validated(() => validateFileOwner(owner));
  const prefix = validated(() => validateFilePrefix(args.prefix));
  const rawCursor = args.cursor;
  const cursor =
    rawCursor === undefined
      ? undefined
      : validated(() => validateFilePath(rawCursor));
  if (cursor !== undefined && !cursor.startsWith(prefix)) {
    throw new SenderError(errors.invalidCursor);
  }
  const limit = args.limit ?? 100;
  if (!Number.isInteger(limit) || limit < 1 || limit > FILE_LIST_PAGE_MAX) {
    throw new SenderError(errors.invalidPageSize);
  }
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
  const files = [];
  for (const row of ctx.db.file.ownerPath.filter([
    owner,
    new Range(from, to),
  ])) {
    files.push({
      id: row.id,
      path: row.path,
      mimeType: row.mimeType,
      size: row.size,
      sha256Hex: row.sha256Hex,
      visibility: row.visibility,
      updatedAt: row.updatedAt,
    });
    if (files.length > limit) break;
  }
  const hasMore = files.length > limit;
  if (hasMore) files.pop();
  return {
    files,
    nextCursor: hasMore ? files[files.length - 1]?.path : undefined,
  };
}

export const readFileBytesParams = {
  path: t.string(),
};

export const readFileBytesReturn = t.object('FileBytes', {
  bytes: t.array(t.u8()),
  mimeType: t.string(),
});

// HTTP handlers never see the caller's identity, so private files are read
// here, from a procedure that knows the sender.
export function readFileBytes(
  ctx: FilesCtx,
  args: InferTypeOfParams<typeof readFileBytesParams>,
  owner: string
): { bytes: number[]; mimeType: string } {
  owner = validated(() => validateFileOwner(owner));
  const path = validated(() => validateFilePath(args.path));
  const row = requireFile(ctx, owner, path);
  const blob = ctx.db.fileBlob.fileId.find(row.id);
  if (!blob) throw new SenderError(errors.notFound);
  return { bytes: blob.bytes, mimeType: row.mimeType };
}

export const setFileVisibilityParams = {
  path: t.string(),
  visibility: t.string(),
};

export function setFileVisibility(
  ctx: FilesCtx,
  args: InferTypeOfParams<typeof setFileVisibilityParams>,
  owner: string
): void {
  owner = validated(() => validateFileOwner(owner));
  const path = validated(() => validateFilePath(args.path));
  const visibility = validVisibility(args.visibility);
  const row = requireFile(ctx, owner, path);
  ctx.db.file.id.update({ ...row, visibility, updatedAt: ctx.timestamp });
}
