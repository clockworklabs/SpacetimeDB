import type {
  HandlerContext,
  InferSchema,
  ProcedureCtx,
  ReducerCtx,
  TransactionCtx,
  ViewCtx,
} from 'spacetimedb/server';
import type spacetimedb from './submodule/index.js';

export type AuthSchema = InferSchema<typeof spacetimedb>;
export type AuthReducerCtx = ReducerCtx<AuthSchema>;
export type AuthProcedureCtx = ProcedureCtx<AuthSchema>;
export type AuthTransactionCtx = TransactionCtx<AuthSchema>;
export type AuthViewCtx = ViewCtx<AuthSchema>;
export type AuthHandlerCtx = HandlerContext<AuthSchema>;
