import * as auth from '@spacetimedb/auth/submodule';
import * as files from '@spacetimedb/files/submodule';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import { schema, table, t, type InferSchema } from 'spacetimedb/server';
import {
  attachment,
  chatUser,
  message,
  messageReaction,
  messageThread,
  presenceEntry,
  room,
  roomActivityEvent,
  roomMember,
  roomReadCursor,
  server,
  serverMember,
  threadMessage,
} from './model';

export const presenceConfig = table(
  { name: 'presence_config', public: false },
  {
    singleton: t.bool().primaryKey(),
    defaultTtlSeconds: t.u32(),
    sweepBatch: t.u32(),
    updatedAt: t.timestamp(),
  }
);

let sweepReducer: unknown;

export const chatSweepTick = table(
  {
    name: 'chat_sweep_tick',
    scheduled: (): any => {
      if (!sweepReducer) {
        throw new Error('chat.sweep_reducer_not_registered');
      }
      return sweepReducer;
    },
  },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

export function setSweepReducer(reducer: unknown): void {
  sweepReducer = reducer;
}

export const spacetimedb = schema({
  auth,
  files,
  rateLimit,
  chatUser,
  server,
  serverMember,
  room,
  roomMember,
  message,
  messageReaction,
  messageThread,
  threadMessage,
  attachment,
  roomReadCursor,
  roomActivityEvent,
  presenceEntry,
  presenceConfig,
  chatSweepTick,
});

export type DbSchema = InferSchema<typeof spacetimedb>;
