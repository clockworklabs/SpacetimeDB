import { t } from 'spacetimedb/server';

export const FILE_VISIBILITY_OWNER = 'owner';
export const FILE_VISIBILITY_PUBLIC = 'public';

// Canonical submodule row shape. Applications with a custom file-like table may
// reuse these fields; standard integrations mount @spacetimedb/files/submodule.
export const fileRow = {
  id: t.u64().primaryKey().autoInc(),
  ownerPathKey: t.string().unique(),
  path: t.string().index(),
  ownerUserId: t.string().index(),
  mimeType: t.string(),
  size: t.u64(),
  sha256Hex: t.string(),
  visibility: t.string().index(),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

export const fileBlobRow = {
  fileId: t.u64().primaryKey(),
  bytes: t.array(t.u8()),
};

export const fileSummary = t.object('FileSummary', {
  id: t.u64(),
  path: t.string(),
  mimeType: t.string(),
  size: t.u64(),
  sha256Hex: t.string(),
  visibility: t.string(),
  updatedAt: t.timestamp(),
});

export const fileListPage = t.object('FileListPage', {
  files: t.array(fileSummary),
  nextCursor: t.option(t.string()),
});
