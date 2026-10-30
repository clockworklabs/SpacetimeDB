import * as auth from '@spacetimedb/auth/submodule';
import * as files from '@spacetimedb/files/submodule';
import * as presence from '@spacetimedb/presence/submodule';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import { schema, table, t, type InferSchema } from 'spacetimedb/server';
import {
  attachment,
  chatUser,
  message,
  messageReaction,
  messageThread,
  room,
  roomActivityEvent,
  roomMember,
  roomReadCursor,
  server,
  serverMember,
  threadMessage,
} from './model';

export const chatSweepTick = table(
  { name: 'chat_sweep_tick' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

export const spacetimedb = schema({
  auth,
  files,
  rateLimit,
  presence,
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
  chatSweepTick,
});

export type DbSchema = InferSchema<typeof spacetimedb>;
