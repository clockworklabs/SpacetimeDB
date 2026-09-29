import {
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema';
import { errors, throwSenderError } from './validation';

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
  if (!isAdmin(ctx, sender)) throwSenderError(errors.notAuthorized);
}

export const addAdminIdentity = spacetimedb.procedure(
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

export const removeAdminIdentity = spacetimedb.procedure(
  { identity: t.identity() },
  t.unit(),
  (ctx: ProcedureModuleCtx, { identity }) => {
    ctx.withTx(tx => {
      requireAdmin(tx, ctx.sender);
      const existing = tx.db.posthogAdminIdentity.identity.find(identity);
      if (!existing) return;
      if (tx.db.posthogAdminIdentity.count() <= 1n) {
        throwSenderError(errors.cannotRemoveLastAdmin);
      }
      tx.db.posthogAdminIdentity.delete(existing);
    });
    return {};
  }
);
