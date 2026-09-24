import * as apiKeys from '@spacetimedb/api-keys/submodule';
import * as grid from '@spacetimedb/grid/submodule';
import {
  schema,
  table,
  t,
  type HandlerContext,
  type InferSchema,
  type ReducerCtx,
  type TransactionCtx,
  type ViewCtx,
} from 'spacetimedb/server';

export const world = table(
  { name: 'world', public: true },
  {
    ownerSubject: t.string().primaryKey(),
    gridId: t.u64().index(),
    name: t.string(),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
  }
);

export const worldEvent = table(
  { name: 'world_event', public: true },
  {
    eventId: t.u64().primaryKey().autoInc(),
    ownerSubject: t.string().index(),
    keyPrefix: t.string(),
    action: t.string().index(),
    allowed: t.bool().index(),
    reason: t.string(),
    message: t.string(),
    createdAt: t.timestamp().index(),
  }
);

export const accessKeySummary = table(
  { name: 'access_key_summary', public: false },
  {
    keyId: t.string().primaryKey(),
    prefix: t.string(),
    ownerSubject: t.string().index(),
    name: t.string(),
    scopesJson: t.string(),
    metadataJson: t.option(t.string()),
    status: apiKeys.apiKeyStatus.index(),
    createdAt: t.timestamp().index(),
    expiresAt: t.option(t.timestamp()),
    lastUsedAt: t.option(t.timestamp()),
    revokedAt: t.option(t.timestamp()),
  }
);

// The public presence table supplies the colony roster and cursors.
export const presenceEntry = table(
  { name: 'presence_entry', public: true },
  {
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
  }
);

export const presenceConfig = table(
  { name: 'presence_config', public: false },
  {
    singleton: t.bool().primaryKey(),
    defaultTtlSeconds: t.u32(),
    sweepBatch: t.u32(),
    updatedAt: t.timestamp(),
  }
);

export const colonySweepTick = table(
  { name: 'colony_sweep_tick' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

export const spacetimedb = schema({
  apiKeys,
  grid,
  world,
  worldEvent,
  accessKeySummary,
  presenceEntry,
  presenceConfig,
  colonySweepTick,
});

export type Schema = InferSchema<typeof spacetimedb>;
export type Tx = ReducerCtx<Schema> | TransactionCtx<Schema>;
export type ReadCtx = Tx | ViewCtx<Schema>;
export type HttpCtx = HandlerContext<Schema>;
