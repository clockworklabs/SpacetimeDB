import type { SyncResponse, Request } from 'spacetimedb/server';
import { buildVerifyEmail } from '../mailer';
import type { AuthHandlerCtx } from '../context';
import type { AuthHttpOptions } from '../client';
import {
  HttpError,
  jsonResponse,
  parseQueryString,
  redirectResponse,
  requireConfig,
} from './http';
import { AUTH_RATE_LIMITS, enforceIpRateLimit } from '../rate_limit';
import { requestSession } from '../sessions';
import { findVerification, newVerification } from './password_reset';

const PURPOSE = 'email_verify';
const TOKEN_TTL_SECONDS = 60n * 60n * 24n;

export function emailVerifyRequest(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions
): SyncResponse {
  const sendMail = options.sendMail;
  if (!sendMail) throw new HttpError('mailer_not_configured', 500);
  const limited = enforceIpRateLimit(
    ctx,
    req,
    AUTH_RATE_LIMITS.emailVerifyRequest,
    options.trustedProxyHeader
  );
  if (limited) return limited;

  const mail = ctx.withTx(tx => {
    const cfg = requireConfig(tx);
    const session = requestSession(tx, cfg, req);
    const user = session && tx.db.authUser.userId.find(session.userId);
    if (!user) throw new HttpError('unauthenticated', 401);
    if (user.emailVerified) return undefined;
    return buildVerifyEmail({
      to: user.email,
      baseUrl: cfg.baseUrl,
      token: newVerification(tx, user.email, PURPOSE, TOKEN_TTL_SECONDS),
      appName: options.appName,
    });
  });
  if (!mail) return jsonResponse({ ok: true, alreadyVerified: true });
  sendMail(ctx, mail);
  return jsonResponse({ ok: true });
}

export function emailVerify(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions
): SyncResponse {
  const token = parseQueryString(req.uri)['token'];
  if (!token) throw new HttpError('missing_token', 400);

  ctx.withTx(tx => {
    const row = findVerification(tx, token, PURPOSE);
    tx.db.authVerification.delete(row);
    const user = tx.db.authUser.email.find(row.identifier);
    if (!user) throw new HttpError('bad_token', 400);
    tx.db.authUser.userId.update({
      ...user,
      emailVerified: true,
      updatedAt: tx.timestamp,
    });
  });

  return redirectResponse(options.emailVerifiedRedirect ?? '/');
}
