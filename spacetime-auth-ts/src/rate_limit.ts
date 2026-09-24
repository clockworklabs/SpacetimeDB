import type { Request, SyncResponse } from 'spacetimedb/server';
import {
  client,
  type RateLimitResult,
} from '@spacetimedb/rate-limit/submodule';
import { errorResponse } from './handlers/http';
import { clientKey, type TrustedProxyHeader } from './request-trust';
import type { AuthHandlerCtx } from './context';
export {
  clientKey,
  type AuthHttpOptions,
  type TrustedProxyHeader,
} from './request-trust';

export type AuthRateLimitPolicy = ReturnType<typeof client>;

export const AUTH_RATE_LIMITS = {
  passwordSignup: client({
    scope: 'auth.password.signup',
    limit: 5,
    windowSeconds: 3600,
  }),
  passwordLoginIp: client({
    scope: 'auth.password.login.ip',
    limit: 30,
    windowSeconds: 300,
  }),
  passwordLoginEmail: client({
    scope: 'auth.password.login.email',
    limit: 10,
    windowSeconds: 300,
  }),
  passwordForgotIp: client({
    scope: 'auth.password.forgot.ip',
    limit: 5,
    windowSeconds: 3600,
  }),
  passwordForgotEmail: client({
    scope: 'auth.password.forgot.email',
    limit: 3,
    windowSeconds: 3600,
  }),
  passwordReset: client({
    scope: 'auth.password.reset',
    limit: 5,
    windowSeconds: 900,
  }),
  oauthStart: client({
    scope: 'auth.oauth.start',
    limit: 30,
    windowSeconds: 300,
  }),
  emailVerifyRequest: client({
    scope: 'auth.email.verify_request',
    limit: 5,
    windowSeconds: 3600,
  }),
} satisfies Record<string, AuthRateLimitPolicy>;

function normalizePart(value: string): string {
  return value.toLowerCase().trim().slice(0, 256);
}

export function rateLimitResponse(result: RateLimitResult): SyncResponse {
  return errorResponse('rate_limited', 429, {
    'retry-after': String(result.retryAfterSeconds),
    'x-ratelimit-limit': String(result.limit),
    'x-ratelimit-remaining': String(result.remaining),
    'x-ratelimit-reset': String(
      Number((result.resetAt.microsSinceUnixEpoch as bigint) / 1_000_000n)
    ),
  });
}

export function enforceRateLimits(
  ctx: AuthHandlerCtx,
  _req: Request,
  checks: Array<{ policy: AuthRateLimitPolicy; actor: string }>
): SyncResponse | null {
  let blocked: RateLimitResult | null = null;
  for (const check of checks) {
    const result = ctx.as.rateLimit.withTx(tx =>
      check.policy.consume(tx, {
        key: normalizePart(check.actor),
      })
    );
    if (!result.allowed) {
      blocked = result;
      break;
    }
  }
  return blocked ? rateLimitResponse(blocked) : null;
}

export function enforceIpRateLimit(
  ctx: AuthHandlerCtx,
  req: Request,
  policy: AuthRateLimitPolicy,
  trustedProxyHeader?: TrustedProxyHeader
): SyncResponse | null {
  const key = clientKey(req, trustedProxyHeader);
  if (!key) return null;
  return enforceRateLimits(ctx, req, [{ policy, actor: `ip:${key}` }]);
}
