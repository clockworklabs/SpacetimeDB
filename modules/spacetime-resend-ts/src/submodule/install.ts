import type { ReducerModuleCtx } from './schema.js';

export function install(ctx: ReducerModuleCtx) {
  if (ctx.db.resendAdminIdentity.identity.find(ctx.sender) != null) return;
  ctx.db.resendAdminIdentity.insert({
    identity: ctx.sender,
    addedAt: ctx.timestamp,
  });
}
