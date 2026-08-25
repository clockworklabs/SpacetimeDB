import type { SyncResponse, Request } from 'spacetimedb/server';
import { hashPassword, randomToken, uuidV7 } from '../crypto.ts';
import {
  buildPasswordResetEmail,
  MailerNotConfiguredError,
  type SendMailFn,
} from '../mailer.ts';
import {
  type AuthHandlerCtx,
  ConfigMissingError,
  errorResponse,
  jsonResponse,
  requireConfig,
  safeJson,
} from './_helpers.ts';
import { Timestamp } from 'spacetimedb';
import {
  AUTH_RATE_LIMITS,
  type AuthHttpOptions,
  clientKey,
  enforceIpRateLimit,
  enforceRateLimits,
} from '../rate_limit.ts';

const PURPOSE = 'password_reset';
const TOKEN_TTL_SECONDS = 60n * 60n;
const MIN_PASSWORD_LEN = 8;
const MAX_PASSWORD_LEN = 1024;
const MAX_EMAIL_LEN = 320;
const MAX_TOKEN_LEN = 256;

interface ForgotBody {
  email: string;
}
interface ResetBody {
  token: string;
  newPassword: string;
}

export interface ForgotPasswordOpts extends AuthHttpOptions {
  sendMail: SendMailFn;
  appName?: string;
}

// Always return 200 to keep account existence private.
export function makeForgotPasswordHandler(opts: ForgotPasswordOpts) {
  return function forgot(ctx: AuthHandlerCtx, req: Request): SyncResponse {
    if (!opts.sendMail) throw new MailerNotConfiguredError();

    const body = safeJson<ForgotBody>(req);
    if (!body?.email) return errorResponse('invalid_request', 400);
    const email = body.email.toLowerCase().trim();
    if (email.length === 0 || email.length > MAX_EMAIL_LEN)
      return errorResponse('invalid_request', 400);
    const ipKey = clientKey(req, opts.trustedProxyHeader);
    const limited = enforceRateLimits(ctx, req, [
      ...(ipKey
        ? [{ policy: AUTH_RATE_LIMITS.passwordForgotIp, actor: `ip:${ipKey}` }]
        : []),
      { policy: AUTH_RATE_LIMITS.passwordForgotEmail, actor: `email:${email}` },
    ]);
    if (limited) return limited;

    const verificationId = uuidV7(
      ctx.random,
      Number(ctx.timestamp.microsSinceUnixEpoch / 1000n)
    );
    const token = randomToken(ctx.random, 32);

    let recipient: string | null = null;
    let baseUrl: string;
    try {
      ({ recipient, baseUrl } = ctx.withTx(tx => {
        const cfg = requireConfig(tx);
        const user = tx.db.authUser.email.find(email);
        if (!user) return { recipient: null, baseUrl: cfg.baseUrl };

        for (const row of tx.db.authVerification.identifier.filter(email)) {
          if (row.purpose === PURPOSE) tx.db.authVerification.delete(row);
        }
        tx.db.authVerification.insert({
          verificationId,
          identifier: email,
          value: token,
          purpose: PURPOSE,
          expiresAt: new Timestamp(
            ctx.timestamp.microsSinceUnixEpoch + TOKEN_TTL_SECONDS * 1_000_000n
          ),
          createdAt: ctx.timestamp,
        });
        return { recipient: email, baseUrl: cfg.baseUrl };
      }));
    } catch (e) {
      if (e instanceof ConfigMissingError)
        return errorResponse('config_missing', 500);
      throw e;
    }

    if (recipient) {
      const mail = buildPasswordResetEmail({
        baseUrl,
        token,
        appName: opts.appName,
      });
      mail.to = recipient;
      opts.sendMail(ctx, mail);
    }
    return jsonResponse({ ok: true });
  };
}

export function resetPasswordHandler(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions = {}
): SyncResponse {
  const body = safeJson<ResetBody>(req);
  if (!body?.token || !body?.newPassword)
    return errorResponse('invalid_request', 400);
  if (body.newPassword.length < MIN_PASSWORD_LEN)
    return errorResponse('password_too_short', 400);
  if (body.newPassword.length > MAX_PASSWORD_LEN)
    return errorResponse('password_too_long', 400);
  if (body.token.length > MAX_TOKEN_LEN) return errorResponse('bad_token', 400);
  const limited = enforceIpRateLimit(
    ctx,
    req,
    AUTH_RATE_LIMITS.passwordReset,
    options.trustedProxyHeader
  );
  if (limited) return limited;

  const newHash = hashPassword(ctx.random, body.newPassword);

  try {
    ctx.withTx(tx => {
      const row = tx.db.authVerification.value.find(body.token);
      if (!row || row.purpose !== PURPOSE) throw new BadTokenError();
      if (
        row.expiresAt.microsSinceUnixEpoch < ctx.timestamp.microsSinceUnixEpoch
      ) {
        tx.db.authVerification.delete(row);
        throw new BadTokenError();
      }

      const user = tx.db.authUser.email.find(row.identifier);
      tx.db.authVerification.delete(row);
      if (!user) throw new BadTokenError();

      let updated = false;
      for (const acct of tx.db.authAccount.userId.filter(user.userId)) {
        if (acct.providerId === 'password') {
          tx.db.authAccount.accountId.update({
            ...acct,
            passwordHash: newHash,
            updatedAt: ctx.timestamp,
          });
          updated = true;
        }
      }
      if (!updated) {
        const accountId = uuidV7(
          ctx.random,
          Number(ctx.timestamp.microsSinceUnixEpoch / 1000n)
        );
        tx.db.authAccount.insert({
          accountId,
          userId: user.userId,
          providerId: 'password',
          providerAccountId: user.email,
          passwordHash: newHash,
          accessToken: undefined,
          refreshToken: undefined,
          accessTokenExpiresAt: undefined,
          createdAt: ctx.timestamp,
          updatedAt: ctx.timestamp,
        });
      }

      for (const s of tx.db.authSession.userId.filter(user.userId)) {
        tx.db.authSession.delete(s);
      }
    });
  } catch (e) {
    if (e instanceof BadTokenError) return errorResponse('bad_token', 400);
    throw e;
  }

  return jsonResponse({ ok: true });
}

class BadTokenError extends Error {
  constructor() {
    super('bad_token');
  }
}
