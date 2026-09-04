import { table, t } from 'spacetimedb/server';

export const presenceEntryRow = {
  key: t.string().primaryKey(),
  scope: t.string().index(),
  subject: t.string().index(),
  status: t.string().index(),
  activity: t.option(t.string()),
  payloadJson: t.option(t.string()),
  joinedAt: t.timestamp().index(),
  lastSeenAt: t.timestamp().index(),
  expiresAt: t.timestamp().index(),
  updatedAt: t.timestamp(),
};

export const presenceConfigRow = {
  singleton: t.bool().primaryKey(),
  defaultTtlSeconds: t.u32(),
  sweepBatch: t.u32(),
  updatedAt: t.timestamp(),
};

export const presenceSweepTickRow = {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
};

export function createPresenceEntryTable(options?: {
  name?: string;
  public?: boolean;
}) {
  return table(
    {
      name: options?.name ?? 'presence_entry',
      public: options?.public ?? false,
    },
    presenceEntryRow
  );
}

export function createPresenceConfigTable(options?: {
  name?: string;
  public?: boolean;
}) {
  return table(
    {
      name: options?.name ?? 'presence_config',
      public: options?.public ?? false,
    },
    presenceConfigRow
  );
}

export const presenceEntryTable = createPresenceEntryTable();
export const presenceConfigTable = createPresenceConfigTable();

export const presenceTables = {
  presenceEntry: presenceEntryTable,
  presenceConfig: presenceConfigTable,
};
