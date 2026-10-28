import { table, t } from 'spacetimedb/server';
import type { Infer } from 'spacetimedb';

const fileOptions = {
  name: 'file',
  indexes: [
    {
      accessor: 'byStateDue',
      algorithm: 'btree',
      columns: ['state', 'nextAttemptAt'],
    } as const,
    {
      accessor: 'byOwnerState',
      algorithm: 'btree',
      columns: ['owner', 'state'],
    } as const,
  ],
};
const fileColumns = {
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
};
export const file: ReturnType<
  typeof table<typeof fileColumns, typeof fileOptions>
> = table(fileOptions, fileColumns);

const ownerUsageOptions = { name: 'owner_usage' } as const;
const ownerUsageColumns = {
  owner: t.string().primaryKey(),
  count: t.u32(),
  bytes: t.u64(),
};
export const ownerUsage: ReturnType<
  typeof table<typeof ownerUsageColumns, typeof ownerUsageOptions>
> = table(ownerUsageOptions, ownerUsageColumns);

const cleanupOptions = { name: 'cleanup' } as const;
const cleanupColumns = {
  id: t.u32().primaryKey(),
  target: t.string(),
  // Scopes object keys, so IDs reused after a database reset never collide.
  installedAt: t.timestamp(),
  after: t.string(),
  nextSweepAt: t.timestamp(),
  claim: t.u64(),
  lastError: t.option(t.string()),
};
export const cleanup: ReturnType<
  typeof table<typeof cleanupColumns, typeof cleanupOptions>
> = table(cleanupOptions, cleanupColumns);

const s3TickOptions = {} as const;
const s3TickColumns = {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
};
export const s3Tick: ReturnType<
  typeof table<typeof s3TickColumns, typeof s3TickOptions>
> = table(s3TickOptions, s3TickColumns);

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
