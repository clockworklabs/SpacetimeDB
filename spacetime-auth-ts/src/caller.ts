// Caller identity helpers. Browser must call link_connection after STDB connect.

import { SenderError } from 'spacetimedb/server';
import type {
  AuthProcedureCtx,
  AuthReducerCtx,
  AuthViewCtx,
} from './context.ts';
import type { AuthUser } from './types.ts';

type CallerContext = AuthReducerCtx | AuthProcedureCtx | AuthViewCtx;

function hasDirectDb(ctx: CallerContext): ctx is AuthReducerCtx | AuthViewCtx {
  return 'db' in ctx;
}

/** Returns the userId bound to ctx.sender, or null if not linked. */
export function getCallerUserId(ctx: CallerContext): string | null {
  if (hasDirectDb(ctx)) {
    const binding = ctx.db.authConnectionBinding.stdbIdentity.find(ctx.sender);
    return binding?.userId ?? null;
  }
  return ctx.withTx(tx => {
    const binding = tx.db.authConnectionBinding.stdbIdentity.find(ctx.sender);
    return binding?.userId ?? null;
  });
}

/** Look up the caller's auth_user row, or null. */
export function findCallerUser(ctx: CallerContext): AuthUser | null {
  if (hasDirectDb(ctx)) {
    const binding = ctx.db.authConnectionBinding.stdbIdentity.find(ctx.sender);
    if (!binding) return null;
    return ctx.db.authUser.userId.find(binding.userId) ?? null;
  }
  return ctx.withTx(tx => {
    const binding = tx.db.authConnectionBinding.stdbIdentity.find(ctx.sender);
    if (!binding) return null;
    return tx.db.authUser.userId.find(binding.userId) ?? null;
  });
}

/** Returns userId. Throws SenderError('auth.not_authenticated') if no binding. */
export function requireCallerUserId(ctx: CallerContext): string {
  const userId = getCallerUserId(ctx);
  if (!userId) throw new SenderError('auth.not_authenticated');
  return userId;
}
