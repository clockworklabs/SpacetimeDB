import { t } from 'spacetimedb/server';

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
