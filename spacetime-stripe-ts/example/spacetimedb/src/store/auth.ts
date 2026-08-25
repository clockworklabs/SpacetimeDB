import {
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema';
import { throwSenderError } from './utils';

// Admin gate. Fresh publishes seed the owner via init. Public reducers never
// bootstrap admin state from "first caller wins".
type Sender = WriteCtx['sender'];
type ModuleTimestamp = WriteCtx['timestamp'];

export type AdminVerdict = 'admin' | 'denied';

export function isAdmin(ctx: WriteCtx, sender: Sender): boolean {
  return ctx.db.storeAdminIdentity.identity.find(sender) != null;
}

export function adminVerdict(ctx: WriteCtx, sender: Sender): AdminVerdict {
  return isAdmin(ctx, sender) ? 'admin' : 'denied';
}

export function denyIfNotAdmin(verdict: AdminVerdict): void {
  if (verdict === 'denied') throwSenderError('store.not_authorized');
}

export function requireAdmin(ctx: WriteCtx, sender: Sender): void {
  if (!isAdmin(ctx, sender)) throwSenderError('store.not_authorized');
}

// For owner-gated repair/setup code only. Do not call from a public bootstrap path.
export function seedAdmin(
  ctx: WriteCtx,
  sender: Sender,
  timestamp: ModuleTimestamp
) {
  if (ctx.db.storeAdminIdentity.identity.find(sender) != null) return;
  ctx.db.storeAdminIdentity.insert({
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
      if (tx.db.storeAdminIdentity.identity.find(identity) == null) {
        tx.db.storeAdminIdentity.insert({
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
      const existing = tx.db.storeAdminIdentity.identity.find(identity);
      if (!existing) return;
      if (tx.db.storeAdminIdentity.count() <= 1n) {
        throwSenderError('store.cannot_remove_last_admin');
      }
      tx.db.storeAdminIdentity.delete(existing);
    });
    return {};
  }
);
