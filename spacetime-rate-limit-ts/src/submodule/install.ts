import { installRateLimitState, type RateLimitInstallOpts } from '../limit';
import type { ReducerModuleCtx } from './schema';

/** Call from the host's init reducer to seed its publishing identity and cleanup timer. */
export function install(ctx: ReducerModuleCtx, opts?: RateLimitInstallOpts) {
  if (ctx.db.rateLimitConfig.singleton.find(true)) return;
  if (ctx.db.rateLimitAdminIdentity.identity.find(ctx.sender) == null) {
    ctx.db.rateLimitAdminIdentity.insert({
      identity: ctx.sender,
      addedAt: ctx.timestamp,
    });
  }
  installRateLimitState(ctx, opts);
}
