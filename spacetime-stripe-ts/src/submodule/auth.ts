import {
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema';
import { throwSenderError } from './validation';

// Admin gate. Fresh publishes seed the owner via init. Public submodule calls
// never bootstrap admin state from "first caller wins".
type Sender = WriteCtx['sender'];
type ModuleTimestamp = WriteCtx['timestamp'];

export type AdminVerdict = 'admin' | 'denied';

export function isAdmin(ctx: WriteCtx, sender: Sender): boolean {
  return ctx.db.stripeAdminIdentity.identity.find(sender) != null;
}

export function adminVerdict(ctx: WriteCtx, sender: Sender): AdminVerdict {
  return isAdmin(ctx, sender) ? 'admin' : 'denied';
}

export function denyIfNotAdmin(verdict: AdminVerdict): void {
  if (verdict === 'denied') throwSenderError('stripe.not_authorized');
}

export function requireAdmin(ctx: WriteCtx, sender: Sender): void {
  if (!isAdmin(ctx, sender)) throwSenderError('stripe.not_authorized');
}

// For owner-gated repair/setup code only. Do not call from a public bootstrap path.
export function seedAdmin(
  ctx: WriteCtx,
  sender: Sender,
  timestamp: ModuleTimestamp
) {
  if (ctx.db.stripeAdminIdentity.identity.find(sender) != null) return;
  ctx.db.stripeAdminIdentity.insert({
    identity: sender,
    addedAtMicros: timestamp.microsSinceUnixEpoch,
  });
}

export const add_admin_identity = spacetimedb.procedure(
  { identity: t.identity() },
  t.unit(),
  (ctx: ProcedureModuleCtx, { identity }) => {
    const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
    denyIfNotAdmin(verdict);
    ctx.withTx(tx => {
      if (tx.db.stripeAdminIdentity.identity.find(identity) == null) {
        tx.db.stripeAdminIdentity.insert({
          identity,
          addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
        });
      }
    });
    return {};
  }
);

export const remove_admin_identity = spacetimedb.procedure(
  { identity: t.identity() },
  t.unit(),
  (ctx: ProcedureModuleCtx, { identity }) => {
    const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
    denyIfNotAdmin(verdict);
    ctx.withTx(tx => {
      const existing = tx.db.stripeAdminIdentity.identity.find(identity);
      if (!existing) return;
      if (tx.db.stripeAdminIdentity.count() <= 1n) {
        throwSenderError('stripe.cannot_remove_last_admin');
      }
      tx.db.stripeAdminIdentity.delete(existing);
    });
    return {};
  }
);
