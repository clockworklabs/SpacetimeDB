import type { ProcedureExport } from 'spacetimedb/server';
import {
  spacetimedb,
  t,
  unitResult,
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

const addAdminIdentityArgs = { identity: t.identity() };
export const addAdminIdentity: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof addAdminIdentityArgs,
  typeof unitResult
> = spacetimedb.procedure(
  addAdminIdentityArgs,
  unitResult,
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

const removeAdminIdentityArgs = { identity: t.identity() };
export const removeAdminIdentity: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof removeAdminIdentityArgs,
  typeof unitResult
> = spacetimedb.procedure(
  removeAdminIdentityArgs,
  unitResult,
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
