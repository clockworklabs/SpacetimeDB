import type { ReducerModuleCtx } from './schema';

export function installPostHog(ctx: ReducerModuleCtx) {
  if (ctx.db.posthogAdminIdentity.identity.find(ctx.sender) != null) return;
  ctx.db.posthogAdminIdentity.insert({
    identity: ctx.sender,
    addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
  });
}
