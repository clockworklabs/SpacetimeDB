import * as apiKeys from '@spacetimedb/api-keys/submodule';
import * as grid from '@spacetimedb/grid/submodule';
import * as presence from '@spacetimedb/presence/submodule';
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

export const spacetimedb = schema({
  apiKeys,
  grid,
  presence,
  world,
  worldEvent,
  accessKeySummary,
});

export type Schema = InferSchema<typeof spacetimedb>;
export type Tx = ReducerCtx<Schema> | TransactionCtx<Schema>;
export type ReadCtx = Tx | ViewCtx<Schema>;
export type HttpCtx = HandlerContext<Schema>;
