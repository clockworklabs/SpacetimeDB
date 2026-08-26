import {
  spacetimedb,
  t,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema';
import { throwSenderError } from './validation';

// Admin gate. Fresh publishes seed the owner via init. Public submodule calls
// never bootstrap admin state from "first caller wins". Procedure callers must
// pass the outer ctx.sender explicitly; transaction ctx may not carry sender.
type Sender = WriteCtx['sender'];
type ModuleTimestamp = WriteCtx['timestamp'];

export type AdminVerdict = 'admin' | 'denied';

export function isAdmin(ctx: WriteCtx, sender: Sender): boolean {
  return ctx.db.resendAdminIdentity.identity.find(sender) != null;
}

export function adminVerdict(ctx: WriteCtx, sender: Sender): AdminVerdict {
  return isAdmin(ctx, sender) ? 'admin' : 'denied';
}

export function denyIfNotAdmin(verdict: AdminVerdict): void {
  if (verdict === 'denied') throwSenderError('resend.not_authorized');
}

export function requireAdmin(ctx: WriteCtx, sender: Sender): void {
  if (!isAdmin(ctx, sender)) throwSenderError('resend.not_authorized');
}

// For owner-gated repair/setup code only. Do not call from a public bootstrap path.
export function seedAdmin(
  ctx: WriteCtx,
  sender: Sender,
  timestamp: ModuleTimestamp
) {
  if (ctx.db.resendAdminIdentity.identity.find(sender) != null) return;
  ctx.db.resendAdminIdentity.insert({
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
      if (tx.db.resendAdminIdentity.identity.find(identity) == null) {
        tx.db.resendAdminIdentity.insert({
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
      const existing = tx.db.resendAdminIdentity.identity.find(identity);
      if (!existing) return;
      if (tx.db.resendAdminIdentity.count() <= 1n) {
        throwSenderError('resend.cannot_remove_last_admin');
      }
      tx.db.resendAdminIdentity.delete(existing);
    });
    return {};
  }
);
