import type { Identity, Timestamp } from 'spacetimedb';
import { SenderError } from 'spacetimedb/server';
import type { AuthTransactionCtx } from './context.ts';

export type AdminVerdict = 'admin' | 'denied';

// Non-throwing read so callers can compute the verdict inside a tx and throw
// outside it. A SenderError thrown inside ctx.withTx surfaces as a fatal
// instance error, not a recoverable rejection.
export function authAdminVerdict(
  tx: AuthTransactionCtx,
  sender: Identity
): AdminVerdict {
  return tx.db.authAdminIdentity.identity.find(sender) != null
    ? 'admin'
    : 'denied';
}

export function denyIfNotAdmin(verdict: AdminVerdict): void {
  if (verdict === 'denied') throw new SenderError('auth.not_authorized');
}

// For owner-gated setup code only. Do not call from a public bootstrap path.
export function seedAuthAdmin(
  tx: AuthTransactionCtx,
  sender: Identity,
  timestamp: Timestamp
): void {
  if (tx.db.authAdminIdentity.identity.find(sender) != null) return;
  tx.db.authAdminIdentity.insert({
    identity: sender,
    addedAtMicros: timestamp.microsSinceUnixEpoch,
  });
}
