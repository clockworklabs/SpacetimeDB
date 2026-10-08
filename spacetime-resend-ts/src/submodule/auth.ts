import {
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema.js';
import { throwSenderError } from './validation.js';
import { errors } from './errors.js';

// Admin gate. Fresh publishes seed the owner via init. Public submodule calls
// never bootstrap admin state from "first caller wins". Procedure callers must
// pass the outer ctx.sender explicitly; transaction ctx may not carry sender.
type Sender = WriteCtx['sender'];

export type AdminVerdict = 'admin' | 'denied';

export function isAdmin(ctx: WriteCtx, sender: Sender): boolean {
  return ctx.db.resendAdminIdentity.identity.find(sender) != null;
}

export function adminVerdict(ctx: WriteCtx, sender: Sender): AdminVerdict {
  return isAdmin(ctx, sender) ? 'admin' : 'denied';
}

export function denyIfNotAdmin(verdict: AdminVerdict): void {
  if (verdict === 'denied') throwSenderError(errors.notAuthorized);
}

export function requireAdmin(ctx: WriteCtx, sender: Sender): void {
  if (!isAdmin(ctx, sender)) throwSenderError(errors.notAuthorized);
}

export const addAdminIdentity = spacetimedb.procedure(
  { identity: t.identity() },
  t.unit(),
  (ctx: ProcedureModuleCtx, { identity }) => {
    const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
    denyIfNotAdmin(verdict);
    ctx.withTx(tx => {
      if (tx.db.resendAdminIdentity.identity.find(identity) == null) {
        tx.db.resendAdminIdentity.insert({
          identity,
          addedAt: ctx.timestamp,
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
    const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
    denyIfNotAdmin(verdict);
    ctx.withTx(tx => {
      const existing = tx.db.resendAdminIdentity.identity.find(identity);
      if (!existing) return;
      if (tx.db.resendAdminIdentity.count() <= 1n) {
        throwSenderError(errors.cannotRemoveLastAdmin);
      }
      tx.db.resendAdminIdentity.delete(existing);
    });
    return {};
  }
);
