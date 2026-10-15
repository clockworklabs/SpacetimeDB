import type { ProcedureExport } from 'spacetimedb/server';
import {
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema.js';
import { errors, throwSenderError } from './validation.js';

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

const unitResult: ReturnType<typeof t.unit> = t.unit();

const addAdminIdentityArgs = { identity: t.identity() };
export const addAdminIdentity: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof addAdminIdentityArgs,
  typeof unitResult
> = spacetimedb.procedure(
  addAdminIdentityArgs,
  unitResult,
  (ctx: ProcedureModuleCtx, { identity }) => {
    ctx.withTx(tx => {
      requireAdmin(tx, ctx.sender);
      if (tx.db.posthogAdminIdentity.identity.find(identity) == null) {
        tx.db.posthogAdminIdentity.insert({
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
