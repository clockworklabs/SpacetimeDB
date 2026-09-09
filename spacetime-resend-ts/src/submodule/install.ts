import type { ReducerModuleCtx } from './schema';

export function installResend(ctx: ReducerModuleCtx) {
  if (ctx.db.resendAdminIdentity.identity.find(ctx.sender) != null) return;
  ctx.db.resendAdminIdentity.insert({
    identity: ctx.sender,
    addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
  });
}
