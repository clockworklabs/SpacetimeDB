import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import {
  schema,
  table,
  t,
  type HandlerContext,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
  type ViewCtx,
} from 'spacetimedb/server';
import { authTables } from './tables.ts';

const authSweeperTick = table(
  { name: 'auth_sweeper_tick' },
  {
    scheduledId: t.u64().primaryKey().autoInc(),
    scheduledAt: t.scheduleAt(),
  }
);

// This schema exists only to derive the context types shared by the package's
// reducer, procedure, view, and HTTP implementations. Runtime modules mount the
// same auth tables and the rate-limit submodule under their own schema.
const _authContextSchema = schema({
  ...authTables,
  authSweeperTick,
  rateLimit,
});

export type AuthSchema = InferSchema<typeof _authContextSchema>;
export type AuthReducerCtx = ReducerCtx<AuthSchema>;
export type AuthProcedureCtx = ProcedureCtx<AuthSchema>;
export type AuthTransactionCtx = TransactionCtx<AuthSchema>;
export type AuthViewCtx = ViewCtx<AuthSchema>;
export type AuthHandlerCtx = HandlerContext<AuthSchema>;
