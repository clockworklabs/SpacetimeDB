import {
  Router,
  SenderError,
  schema,
  table,
  t,
  type InferSchema,
  type ReducerCtx,
  type ViewCtx,
} from 'spacetimedb/server';
import {
  FILE_VISIBILITY_OWNER,
  FILE_VISIBILITY_PUBLIC,
  FILE_BYTES_MAX,
  fileSummary,
  fileSha256Hex,
  makeFileServeImpl,
  ownerPathKey,
  readFileBytesParams,
  readFileBytesReturn,
  readFileBytesImpl,
  validateMimeType,
} from '@spacetimedb/files/submodule';
import * as files from '@spacetimedb/files/submodule';

const PATH_MAX = 1024;
const NAME_MAX = 128;
const VALID_VISIBILITIES = new Set([
  FILE_VISIBILITY_OWNER,
  FILE_VISIBILITY_PUBLIC,
]);

const folder = table(
  {
    name: 'folder',
    public: false,
    indexes: [
      {
        accessor: 'ownerPath',
        algorithm: 'btree',
        columns: ['ownerUserId', 'path'] as const,
      },
    ] as const,
  },
  {
    // Reducers enforce per-owner folder uniqueness.
    id: t.u64().primaryKey().autoInc(),
    ownerUserId: t.string().index(),
    path: t.string(),
    name: t.string(),
    parentPath: t.string().index(),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
  }
);

const spacetimedb = schema({
  files,
  folder,
});
export default spacetimedb;

type Schema = InferSchema<typeof spacetimedb>;
type Tx = ReducerCtx<Schema>;

const FolderRow = folder.rowType;

function senderError(message: string): never {
  throw new SenderError(message);
}

function ownerUserId(ctx: { sender: { toHexString(): string } }): string {
  return ctx.sender.toHexString();
}

function normalizePath(input: string, kind: 'file' | 'folder'): string {
  let path = input.trim().replace(/\\/g, '/').replace(/\/+/g, '/');
  if (!path.startsWith('/')) path = `/${path}`;
  if (path.length > 1 && path.endsWith('/'))
    senderError('vault.invalid_path:trailing_slash');
  if (path.length === 0 || path.length > PATH_MAX)
    senderError('vault.invalid_path:length');
  if (kind === 'file' && path === '/') senderError('vault.invalid_file_path');
  const parts = path.split('/').filter(Boolean);
  for (const part of parts) {
    if (part === '.' || part === '..')
      senderError('vault.invalid_path:segment');
    if (part.trim() !== part || part.length === 0 || part.length > NAME_MAX) {
      senderError('vault.invalid_path:segment');
    }
  }
  return path;
}

function parentPathFor(path: string): string {
  if (path === '/') return '/';
  const idx = path.lastIndexOf('/');
  return idx <= 0 ? '/' : path.slice(0, idx);
}

function basename(path: string): string {
  if (path === '/') return '/';
  return path.slice(path.lastIndexOf('/') + 1);
}

function findOwnedFolder(tx: Tx, path: string, owner: string) {
  for (const row of tx.db.folder.ownerPath.filter([owner, path])) return row;
  return undefined;
}

function assertParentFolderExists(tx: Tx, path: string, owner: string): void {
  const parent = parentPathFor(path);
  if (parent === '/') return;
  if (!findOwnedFolder(tx, parent, owner))
    senderError(`vault.parent_not_found:${parent}`);
}

function assertNoFolderCollision(tx: Tx, path: string, owner: string): void {
  if (findOwnedFolder(tx, path, owner))
    senderError(`vault.folder_exists:${path}`);
}

function assertNoOwnedFileCollision(tx: Tx, path: string, owner: string): void {
  if (tx.db.files.file.ownerPathKey.find(ownerPathKey(owner, path))) {
    senderError(`vault.file_exists:${path}`);
  }
}

function requireOwnedFolder(tx: Tx, path: string, owner: string) {
  const row = findOwnedFolder(tx, path, owner);
  if (!row) senderError(`vault.folder_not_found:${path}`);
  return row;
}

function requireOwnedFile(tx: Tx, path: string, owner: string) {
  const row = tx.db.files.file.ownerPathKey.find(ownerPathKey(owner, path));
  if (!row) senderError(`vault.file_not_found:${path}`);
  return row;
}

function childPrefix(path: string): string {
  return path === '/' ? '/' : `${path}/`;
}

function folderHasChildren(tx: Tx, path: string, owner: string): boolean {
  for (const row of tx.db.folder.parentPath.filter(path)) {
    if (row.ownerUserId === owner) return true;
  }
  const prefix = childPrefix(path);
  for (const row of tx.db.files.file.ownerUserId.filter(owner)) {
    if (row.path.startsWith(prefix)) return true;
  }
  return false;
}

function renameOwnedFile(
  tx: Tx,
  owner: string,
  oldPath: string,
  newPath: string
): void {
  if (oldPath === newPath) return;
  const row = requireOwnedFile(tx, oldPath, owner);
  assertParentFolderExists(tx, newPath, owner);
  assertNoFolderCollision(tx, newPath, owner);
  assertNoOwnedFileCollision(tx, newPath, owner);
  tx.db.files.file.id.update({
    ...row,
    ownerPathKey: ownerPathKey(owner, newPath),
    path: newPath,
    updatedAt: tx.timestamp,
  });
}

export const myFolders = spacetimedb.view(
  { name: 'my_folders', public: true },
  t.array(FolderRow),
  (ctx: ViewCtx<Schema>) => {
    const owner = ownerUserId(ctx);
    return [...ctx.db.folder.ownerUserId.filter(owner)].sort((a, b) =>
      a.path.localeCompare(b.path)
    );
  }
);

export const myFileSummaries = spacetimedb.view(
  { name: 'my_file_summaries', public: true },
  t.array(fileSummary),
  (ctx: ViewCtx<Schema>) => {
    const owner = ownerUserId(ctx);
    const out = [];
    for (const row of ctx.db.files.file.ownerUserId.filter(owner)) {
      out.push({
        id: row.id,
        path: row.path,
        mimeType: row.mimeType,
        size: row.size,
        sha256Hex: row.sha256Hex,
        visibility: row.visibility,
        updatedAt: row.updatedAt,
      });
    }
    out.sort((a, b) => a.path.localeCompare(b.path));
    return out;
  }
);

export const create_folder = spacetimedb.reducer(
  { path: t.string() },
  (ctx, args) => {
    const owner = ownerUserId(ctx);
    const path = normalizePath(args.path, 'folder');
    if (path === '/') return;
    assertParentFolderExists(ctx, path, owner);
    assertNoFolderCollision(ctx, path, owner);
    assertNoOwnedFileCollision(ctx, path, owner);
    ctx.db.folder.insert({
      id: 0n,
      path,
      ownerUserId: owner,
      name: basename(path),
      parentPath: parentPathFor(path),
      createdAt: ctx.timestamp,
      updatedAt: ctx.timestamp,
    });
  }
);

export const delete_folder = spacetimedb.reducer(
  { path: t.string() },
  (ctx, args) => {
    const owner = ownerUserId(ctx);
    const path = normalizePath(args.path, 'folder');
    if (path === '/') senderError('vault.cannot_delete_root');
    const row = requireOwnedFolder(ctx, path, owner);
    if (folderHasChildren(ctx, path, owner))
      senderError(`vault.folder_not_empty:${path}`);
    ctx.db.folder.delete(row);
  }
);

export const rename_folder = spacetimedb.reducer(
  { path: t.string(), newName: t.string() },
  (ctx, args) => {
    const owner = ownerUserId(ctx);
    const oldPath = normalizePath(args.path, 'folder');
    if (oldPath === '/') senderError('vault.cannot_rename_root');
    const newName = args.newName.trim();
    if (newName.length === 0 || newName.includes('/'))
      senderError('vault.invalid_path:segment');
    const parent = parentPathFor(oldPath);
    const newPath = parent === '/' ? `/${newName}` : `${parent}/${newName}`;
    // Require the name to survive path normalization unchanged.
    if (normalizePath(newPath, 'folder') !== newPath)
      senderError('vault.invalid_path:segment');
    if (newPath === oldPath) return;

    const row = requireOwnedFolder(ctx, oldPath, owner);
    assertNoFolderCollision(ctx, newPath, owner);
    assertNoOwnedFileCollision(ctx, newPath, owner);

    // Validate every re-pathed descendant before mutating anything.
    const prefix = childPrefix(oldPath);
    const rePath = (p: string) => newPath + p.slice(oldPath.length);
    const childFolders = [...ctx.db.folder.ownerUserId.filter(owner)].filter(
      f => f.path.startsWith(prefix)
    );
    const childFiles = [...ctx.db.files.file.ownerUserId.filter(owner)].filter(
      f => f.path.startsWith(prefix)
    );
    for (const f of childFolders) {
      const p = rePath(f.path);
      if (p.length > PATH_MAX) senderError('vault.invalid_path:length');
      assertNoOwnedFileCollision(ctx, p, owner);
    }
    for (const f of childFiles) {
      const p = rePath(f.path);
      if (p.length > PATH_MAX) senderError('vault.invalid_path:length');
      assertNoOwnedFileCollision(ctx, p, owner);
    }

    ctx.db.folder.id.update({
      ...row,
      path: newPath,
      name: newName,
      updatedAt: ctx.timestamp,
    });
    for (const f of childFolders) {
      const p = rePath(f.path);
      ctx.db.folder.id.update({
        ...f,
        path: p,
        parentPath: parentPathFor(p),
        updatedAt: ctx.timestamp,
      });
    }
    for (const f of childFiles) {
      const path = rePath(f.path);
      ctx.db.files.file.id.update({
        ...f,
        ownerPathKey: ownerPathKey(owner, path),
        path,
        updatedAt: ctx.timestamp,
      });
    }
  }
);

export const upload_file = spacetimedb.reducer(
  {
    path: t.string(),
    mimeType: t.string(),
    bytes: t.array(t.u8()),
    visibility: t.string(),
  },
  (ctx, args) => {
    const owner = ownerUserId(ctx);
    const path = normalizePath(args.path, 'file');
    if (!VALID_VISIBILITIES.has(args.visibility))
      senderError(`vault.invalid_visibility:${args.visibility}`);
    if (args.bytes.length > FILE_BYTES_MAX)
      senderError(`files.too_large:${args.bytes.length}/${FILE_BYTES_MAX}`);
    assertParentFolderExists(ctx, path, owner);
    assertNoFolderCollision(ctx, path, owner);
    const key = ownerPathKey(owner, path);
    const existing = ctx.db.files.file.ownerPathKey.find(key);
    let mimeType: string;
    try {
      mimeType = validateMimeType(args.mimeType || 'application/octet-stream');
    } catch (error) {
      senderError(
        error instanceof Error ? error.message : 'files.invalid_mime_type'
      );
    }
    const sha256Hex = fileSha256Hex(args.bytes);
    if (existing) {
      ctx.db.files.file.id.update({
        ...existing,
        mimeType,
        size: BigInt(args.bytes.length),
        sha256Hex,
        visibility: args.visibility,
        updatedAt: ctx.timestamp,
      });
      ctx.db.files.fileBlob.fileId.update({
        fileId: existing.id,
        bytes: args.bytes,
      });
      return;
    }
    const row = ctx.db.files.file.insert({
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
    ctx.db.files.fileBlob.insert({ fileId: row.id, bytes: args.bytes });
  }
);

export const delete_file = spacetimedb.reducer(
  { path: t.string() },
  (ctx, args) => {
    const owner = ownerUserId(ctx);
    const path = normalizePath(args.path, 'file');
    const row = ctx.db.files.file.ownerPathKey.find(ownerPathKey(owner, path));
    if (!row) return;
    const blob = ctx.db.files.fileBlob.fileId.find(row.id);
    if (blob) ctx.db.files.fileBlob.delete(blob);
    ctx.db.files.file.id.delete(row.id);
  }
);

export const rename_file = spacetimedb.reducer(
  { oldPath: t.string(), newPath: t.string() },
  (ctx, args) => {
    const owner = ownerUserId(ctx);
    const oldPath = normalizePath(args.oldPath, 'file');
    const newPath = normalizePath(args.newPath, 'file');
    renameOwnedFile(ctx, owner, oldPath, newPath);
  }
);

export const move_file = spacetimedb.reducer(
  { oldPath: t.string(), targetFolderPath: t.string() },
  (ctx, args) => {
    const targetFolderPath = normalizePath(args.targetFolderPath, 'folder');
    const oldPath = normalizePath(args.oldPath, 'file');
    const filename = basename(oldPath);
    const newPath =
      targetFolderPath === '/'
        ? `/${filename}`
        : `${targetFolderPath}/${filename}`;
    renameOwnedFile(ctx, ownerUserId(ctx), oldPath, newPath);
  }
);

export const set_file_visibility = spacetimedb.reducer(
  { path: t.string(), visibility: t.string() },
  (ctx, args) => {
    if (!VALID_VISIBILITIES.has(args.visibility))
      senderError(`files.invalid_visibility:${args.visibility}`);
    const owner = ownerUserId(ctx);
    const path = normalizePath(args.path, 'file');
    const row = ctx.db.files.file.ownerPathKey.find(ownerPathKey(owner, path));
    if (!row) senderError(`files.not_found:${path}`);
    ctx.db.files.file.id.update({
      ...row,
      visibility: args.visibility,
      updatedAt: ctx.timestamp,
    });
  }
);

// Private bytes travel over the authenticated connection. HTTP handlers
// never see the caller's identity.
export const read_file_bytes = spacetimedb.procedure(
  readFileBytesParams,
  readFileBytesReturn,
  (ctx, args) =>
    readFileBytesImpl(
      ctx,
      { path: normalizePath(args.path, 'file') },
      ctx.sender.toHexString()
    )
);

const fileServeImpl = makeFileServeImpl({
  getOwner: ctx => ctx.identity?.toHexString?.(),
});

export const file_serve = spacetimedb.httpHandler((ctx, req) => {
  return fileServeImpl(ctx, req);
});

export const router = spacetimedb.httpRouter(
  new Router().get('/files', file_serve).head('/files', file_serve)
);
