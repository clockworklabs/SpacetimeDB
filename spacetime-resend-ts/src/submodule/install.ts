import type { ReducerModuleCtx } from './schema';

export function install(ctx: ReducerModuleCtx) {
  if (ctx.db.resendAdminIdentity.identity.find(ctx.sender) != null) return;
  ctx.db.resendAdminIdentity.insert({
    identity: ctx.sender,
    addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
  });
}
