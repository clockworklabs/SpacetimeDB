import type { ReducerModuleCtx } from './schema';

export function installApiKeys(ctx: ReducerModuleCtx) {
  if (ctx.db.apiKeyAdminIdentity.identity.find(ctx.sender) != null) return;
  ctx.db.apiKeyAdminIdentity.insert({
    identity: ctx.sender,
    addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
  });
}
