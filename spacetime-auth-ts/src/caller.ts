// Browser clients must call link_connection after connecting to SpacetimeDB.

import type { Identity, Timestamp } from 'spacetimedb';
import { SenderError } from 'spacetimedb/server';
import type {
  AuthProcedureCtx,
  AuthReducerCtx,
  AuthViewCtx,
} from './context.js';
import { errors } from './errors.js';
import type { AuthUser } from './types.js';

type CallerContext = AuthReducerCtx | AuthProcedureCtx | AuthViewCtx;

// Views have no timestamp. The sweep deletes expired sessions and their
// bindings, so a view stops seeing an expired session within a sweep interval.
function boundUser(
  db: AuthViewCtx['db'],
  sender: Identity,
  now: Timestamp | undefined
): AuthUser | null {
  const binding = db.authConnectionBinding.stdbIdentity.find(sender);
  if (!binding) return null;
  const session = db.authSession.sessionId.find(binding.sessionId);
  if (!session) return null;
  if (now && session.expiresAt.microsSinceUnixEpoch <= now.microsSinceUnixEpoch)
    return null;
  return db.authUser.userId.find(binding.userId) ?? null;
}

/** The caller's auth_user row if its connection is bound to a live session. */
export function findCallerUser(ctx: CallerContext): AuthUser | null {
  if ('withTx' in ctx) {
    return ctx.withTx(tx => boundUser(tx.db, ctx.sender, tx.timestamp));
  }
  return boundUser(
    ctx.db,
    ctx.sender,
    'timestamp' in ctx ? ctx.timestamp : undefined
  );
}

/** The caller's userId if its connection is bound to a live session. */
export function getCallerUserId(ctx: CallerContext): string | null {
  return findCallerUser(ctx)?.userId ?? null;
}

/** Returns userId. Throws SenderError(errors.notAuthenticated) otherwise. */
export function requireCallerUserId(ctx: CallerContext): string {
  const userId = getCallerUserId(ctx);
  if (!userId) throw new SenderError(errors.notAuthenticated);
  return userId;
}
