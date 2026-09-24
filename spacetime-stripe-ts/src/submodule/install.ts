import type { ReducerModuleCtx } from './schema';

export function install(ctx: ReducerModuleCtx) {
  if (ctx.db.stripeAdminIdentity.identity.find(ctx.sender) != null) return;
  ctx.db.stripeAdminIdentity.insert({
    identity: ctx.sender,
    addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
  });
}
