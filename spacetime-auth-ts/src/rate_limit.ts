import type { Request, SyncResponse } from 'spacetimedb/server';
import {
  consumeRateLimit,
  type RateLimitResult,
} from '@spacetimedb/rate-limit/submodule';
import { errorResponse } from './handlers/http.ts';
import { clientKey, type TrustedProxyHeader } from './request-trust.ts';
import type { AuthHandlerCtx } from './context.ts';
export {
  clientKey,
  type AuthHttpOptions,
  type TrustedProxyHeader,
} from './request-trust.ts';

export interface AuthRateLimitPolicy {
  scope: string;
  limit: number;
  windowSeconds: number;
}

export const AUTH_RATE_LIMITS = {
  passwordSignup: {
    scope: 'auth.password.signup',
    limit: 5,
    windowSeconds: 3600,
  },
  passwordLoginIp: {
    scope: 'auth.password.login.ip',
    limit: 30,
    windowSeconds: 300,
  },
  passwordLoginEmail: {
    scope: 'auth.password.login.email',
    limit: 10,
    windowSeconds: 300,
  },
  passwordForgotIp: {
    scope: 'auth.password.forgot.ip',
    limit: 5,
    windowSeconds: 3600,
  },
  passwordForgotEmail: {
    scope: 'auth.password.forgot.email',
    limit: 3,
    windowSeconds: 3600,
  },
  passwordReset: { scope: 'auth.password.reset', limit: 5, windowSeconds: 900 },
  oauthStart: { scope: 'auth.oauth.start', limit: 30, windowSeconds: 300 },
  emailVerifyRequest: {
    scope: 'auth.email.verify_request',
    limit: 5,
    windowSeconds: 3600,
  },
} satisfies Record<string, AuthRateLimitPolicy>;

function normalizePart(value: string): string {
  return value.toLowerCase().trim().slice(0, 256);
}

export function rateLimitKey(
  policy: AuthRateLimitPolicy,
  actor: string
): string {
  return `${policy.scope}:${normalizePart(actor)}`;
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
      consumeRateLimit(tx, {
        key: rateLimitKey(check.policy, check.actor),
        scope: check.policy.scope,
        limit: check.policy.limit,
        windowSeconds: check.policy.windowSeconds,
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
