import type { SyncResponse, Request } from 'spacetimedb/server';
import { Timestamp } from 'spacetimedb';
import { hashPassword } from '../crypto';
import { buildPasswordResetEmail } from '../mailer';
import type { AuthHandlerCtx, AuthTransactionCtx } from '../context';
import type { AuthHttpOptions } from '../client';
import { HttpError, jsonResponse, requireConfig, safeJson } from './http';
import {
  AUTH_RATE_LIMITS,
  enforceIpRateLimit,
  enforceRateLimits,
} from '../rate_limit';
import { clientKey } from '../request-trust';
import { deleteSession } from '../sessions';
import { newId, newToken } from '../tokens';
import {
  MAX_EMAIL_LEN,
  MAX_PASSWORD_LEN,
  MIN_PASSWORD_LEN,
  newSalt,
  passwordAccount,
} from './password';

const PURPOSE = 'password_reset';
const TOKEN_TTL_SECONDS = 60n * 60n;
const MAX_TOKEN_LEN = 256;

interface ForgotBody {
  email: string;
}
interface ResetBody {
  token: string;
  newPassword: string;
}

/** The unexpired verification row for a one-time token. */
export function findVerification(
  tx: AuthTransactionCtx,
  token: string,
  purpose: string
) {
  const row = tx.db.authVerification.value.find(token);
  if (
    !row ||
    row.purpose !== purpose ||
    row.expiresAt.microsSinceUnixEpoch < tx.timestamp.microsSinceUnixEpoch
  ) {
    throw new HttpError('bad_token', 400);
  }
  return row;
}

/** Replaces any pending token for this purpose and returns a new one. */
export function newVerification(
  tx: AuthTransactionCtx,
  email: string,
  purpose: string,
  ttlSeconds: bigint
): string {
  for (const row of [...tx.db.authVerification.identifier.filter(email)]) {
    if (row.purpose === purpose) tx.db.authVerification.delete(row);
  }
  const value = newToken(tx);
  tx.db.authVerification.insert({
    verificationId: newId(tx),
    identifier: email,
    value,
    purpose,
    expiresAt: new Timestamp(
      tx.timestamp.microsSinceUnixEpoch + ttlSeconds * 1_000_000n
    ),
    createdAt: tx.timestamp,
  });
  return value;
}

// Always return 200 to keep account existence private.
export function forgotPassword(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions
): SyncResponse {
  const sendMail = options.sendMail;
  if (!sendMail) throw new HttpError('mailer_not_configured', 500);

  const body = safeJson<ForgotBody>(req);
  if (!body?.email) throw new HttpError('invalid_request', 400);
  const email = body.email.toLowerCase().trim();
  if (email.length === 0 || email.length > MAX_EMAIL_LEN)
    throw new HttpError('invalid_request', 400);
  const ipKey = clientKey(req, options.trustedProxyHeader);
  const limited = enforceRateLimits(ctx, [
    ...(ipKey
      ? [{ policy: AUTH_RATE_LIMITS.passwordForgotIp, actor: `ip:${ipKey}` }]
      : []),
    { policy: AUTH_RATE_LIMITS.passwordForgotEmail, actor: `email:${email}` },
  ]);
  if (limited) return limited;

  const mail = ctx.withTx(tx => {
    const cfg = requireConfig(tx);
    if (!tx.db.authUser.email.find(email)) return undefined;
    return buildPasswordResetEmail({
      to: email,
      baseUrl: cfg.baseUrl,
      token: newVerification(tx, email, PURPOSE, TOKEN_TTL_SECONDS),
      appName: options.appName,
    });
  });
  if (mail) sendMail(ctx, mail);
  return jsonResponse({ ok: true });
}

export function resetPassword(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions
): SyncResponse {
  const body = safeJson<ResetBody>(req);
  if (!body?.token || !body?.newPassword)
    throw new HttpError('invalid_request', 400);
  if (body.newPassword.length < MIN_PASSWORD_LEN)
    throw new HttpError('password_too_short', 400);
  if (body.newPassword.length > MAX_PASSWORD_LEN)
    throw new HttpError('password_too_long', 400);
  if (body.token.length > MAX_TOKEN_LEN) throw new HttpError('bad_token', 400);
  const limited = enforceIpRateLimit(
    ctx,
    req,
    AUTH_RATE_LIMITS.passwordReset,
    options.trustedProxyHeader
  );
  if (limited) return limited;

  // Check the token before the slow hash.
  const salt = ctx.withTx(tx => {
    findVerification(tx, body.token, PURPOSE);
    return newSalt(tx);
  });
  const passwordHash = hashPassword(body.newPassword, salt);

  ctx.withTx(tx => {
    const row = findVerification(tx, body.token, PURPOSE);
    tx.db.authVerification.delete(row);
    const user = tx.db.authUser.email.find(row.identifier);
    if (!user) throw new HttpError('bad_token', 400);

    const acct = passwordAccount(tx, user.userId);
    if (acct) {
      tx.db.authAccount.accountId.update({
        ...acct,
        passwordHash,
        updatedAt: tx.timestamp,
      });
    } else {
      tx.db.authAccount.insert({
        accountId: newId(tx),
        userId: user.userId,
        providerId: 'password',
        providerAccountId: user.email,
        passwordHash,
        accessToken: undefined,
        refreshToken: undefined,
        accessTokenExpiresAt: undefined,
        createdAt: tx.timestamp,
        updatedAt: tx.timestamp,
      });
    }
    // The reset link proved control of the mailbox.
    if (!user.emailVerified) {
      tx.db.authUser.userId.update({
        ...user,
        emailVerified: true,
        updatedAt: tx.timestamp,
      });
    }
    for (const s of [...tx.db.authSession.userId.filter(user.userId)]) {
      deleteSession(tx, s);
    }
  });

  return jsonResponse({ ok: true });
}
