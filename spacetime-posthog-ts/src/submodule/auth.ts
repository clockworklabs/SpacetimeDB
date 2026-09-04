import {
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema';
import { throwSenderError } from './validation';

type Sender = WriteCtx['sender'];
type AdminReadableCtx = {
  db: {
    posthogAdminIdentity: {
      identity: { find(identity: Sender): unknown };
    };
  };
};

export function isAdmin(ctx: AdminReadableCtx, sender: Sender): boolean {
  return ctx.db.posthogAdminIdentity.identity.find(sender) != null;
}

export function requireAdmin(ctx: WriteCtx, sender: Sender): void {
  if (!isAdmin(ctx, sender)) throwSenderError('posthog.not_authorized');
}

export const add_admin_identity = spacetimedb.procedure(
  { identity: t.identity() },
  t.unit(),
  (ctx: ProcedureModuleCtx, { identity }) => {
    ctx.withTx(tx => {
      requireAdmin(tx, ctx.sender);
      if (tx.db.posthogAdminIdentity.identity.find(identity) == null) {
        tx.db.posthogAdminIdentity.insert({
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
    ctx.withTx(tx => {
      requireAdmin(tx, ctx.sender);
      const existing = tx.db.posthogAdminIdentity.identity.find(identity);
      if (!existing) return;
      if (tx.db.posthogAdminIdentity.count() <= 1n) {
        throwSenderError('posthog.cannot_remove_last_admin');
      }
      tx.db.posthogAdminIdentity.delete(existing);
    });
    return {};
  }
);
