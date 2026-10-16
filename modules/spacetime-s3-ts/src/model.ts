import { table, t } from 'spacetimedb/server';
import type { Infer } from 'spacetimedb';

export const file = table(
  {
    name: 'file',
    indexes: [
      {
        accessor: 'byStateDue',
        algorithm: 'btree',
        columns: ['state', 'nextAttemptAt'],
      },
      {
        accessor: 'byOwnerState',
        algorithm: 'btree',
        columns: ['owner', 'state'],
      },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    owner: t.string(),
    requestKey: t.string().unique(),
    filename: t.string(),
    contentType: t.string(),
    size: t.u32(),
    checksum: t.string(),
    state: t.enum('FileState', {
      Pending: t.unit(),
      Copying: t.unit(),
      Ready: t.unit(),
      Deleting: t.unit(),
      Deleted: t.unit(),
    }),
    createdAt: t.timestamp(),
    uploadExpiresAt: t.timestamp(),
    nextAttemptAt: t.timestamp(),
    attempt: t.u64(),
    lastError: t.option(t.string()),
  }
);
export const ownerUsage = table(
  { name: 'owner_usage' },
  { owner: t.string().primaryKey(), count: t.u32(), bytes: t.u64() }
);
export const cleanup = table(
  { name: 'cleanup' },
  {
    id: t.u32().primaryKey(),
    target: t.string(),
    // Scopes object keys, so IDs reused after a database reset never collide.
    installedAt: t.timestamp(),
    after: t.string(),
    nextSweepAt: t.timestamp(),
    claim: t.u64(),
    lastError: t.option(t.string()),
  }
);
export const s3Tick = table(
  {},
  { scheduledId: t.u64().primaryKey().autoInc(), scheduledAt: t.scheduleAt() }
);
const uploadGrant = t.object('S3UploadGrant', {
  url: t.string(),
  fields: t.array(
    t.object('S3UploadField', { name: t.string(), value: t.string() })
  ),
  expiresAt: t.timestamp(),
});
export const uploadReservation = t.object('S3UploadReservation', {
  id: t.string(),
  upload: t.option(uploadGrant),
});
export const downloadGrant = t.object('S3DownloadGrant', {
  url: t.string(),
  expiresAt: t.timestamp(),
});
export type FileRecord = Infer<typeof file.rowType>;
