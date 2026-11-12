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

// The key each holder connection joined a colony with. memberKey is
// `${identity hex}/${colony owner subject}`.
export const colonyMember = table(
  { name: 'colony_member', public: false },
  {
    memberKey: t.string().primaryKey(),
    keyId: t.string(),
    prefix: t.string(),
  }
);

export const spacetimedb = schema({
  apiKeys,
  grid,
  presence,
  world,
  worldEvent,
  colonyMember,
});

export type Schema = InferSchema<typeof spacetimedb>;
export type Tx = ReducerCtx<Schema> | TransactionCtx<Schema>;
export type ReadCtx = Tx | ViewCtx<Schema>;
export type HttpCtx = HandlerContext<Schema>;
