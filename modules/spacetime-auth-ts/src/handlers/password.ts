import type { SyncResponse, Request } from 'spacetimedb/server';
import {
  DUMMY_PASSWORD_HASH,
  hashPassword,
  verifyPassword,
} from '../crypto.js';
import type { AuthHandlerCtx, AuthTransactionCtx } from '../context.js';
import type { AuthHttpOptions } from '../client.js';
import { HttpError, jsonResponse, requireConfig, safeJson } from './http.js';
import { AUTH_RATE_LIMITS, enforceRateLimits } from '../rate_limit.js';
import { clientKey } from '../request-trust.js';
import { issueSession } from '../sessions.js';
import { newId, secretBytes } from '../tokens.js';

interface SignupBody {
  email: string;
  password: string;
  name?: string;
}

interface LoginBody {
  email: string;
  password: string;
}

export const MIN_PASSWORD_LEN = 8;
export const MAX_PASSWORD_LEN = 1024;
export const MAX_EMAIL_LEN = 320;
const MAX_NAME_LEN = 128;

export function passwordAccount(tx: AuthTransactionCtx, userId: string) {
  for (const a of tx.db.authAccount.userId.filter(userId)) {
    if (a.providerId === 'password') return a;
  }
  return undefined;
}

export function newSalt(tx: AuthTransactionCtx): Uint8Array {
  return secretBytes(tx).slice(0, 16);
}

export function passwordSignup(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions
): SyncResponse {
  const body = safeJson<SignupBody>(req);
  if (!body?.email || !body?.password)
    throw new HttpError('invalid_request', 400);
  if (body.password.length < MIN_PASSWORD_LEN)
    throw new HttpError('password_too_short', 400);
  if (body.password.length > MAX_PASSWORD_LEN)
    throw new HttpError('password_too_long', 400);
  const email = body.email.toLowerCase().trim();
  if (email.length === 0 || email.length > MAX_EMAIL_LEN)
    throw new HttpError('invalid_email', 400);
  if (body.name !== undefined && body.name.length > MAX_NAME_LEN)
    throw new HttpError('name_too_long', 400);
  const ipKey = clientKey(req, options.trustedProxyHeader);
  const limited = enforceRateLimits(ctx, [
    { policy: AUTH_RATE_LIMITS.passwordSignup, actor: `email:${email}` },
    ...(ipKey
      ? [{ policy: AUTH_RATE_LIMITS.passwordSignup, actor: `ip:${ipKey}` }]
      : []),
  ]);
  if (limited) return limited;

  const assertEmailFree = (tx: AuthTransactionCtx) => {
    if (tx.db.authUser.email.find(email) != null)
      throw new HttpError('email_taken', 409);
  };
  const salt = ctx.withTx(tx => {
    assertEmailFree(tx);
    return newSalt(tx);
  });
  const passwordHash = hashPassword(body.password, salt);

  const out = ctx.withTx(tx => {
    const cfg = requireConfig(tx);
    assertEmailFree(tx);
    const userId = newId(tx);
    tx.db.authUser.insert({
      userId,
      email,
      emailVerified: false,
      name: body.name,
      image: undefined,
      createdAt: tx.timestamp,
      updatedAt: tx.timestamp,
    });
    tx.db.authAccount.insert({
      accountId: newId(tx),
      userId,
      providerId: 'password',
      providerAccountId: email,
      passwordHash,
      accessToken: undefined,
      refreshToken: undefined,
      accessTokenExpiresAt: undefined,
      createdAt: tx.timestamp,
      updatedAt: tx.timestamp,
    });
    return { userId, ...issueSession(tx, cfg, userId, req, options) };
  });

  return jsonResponse(
    { user: { userId: out.userId, email }, token: out.token },
    200,
    [out.cookie]
  );
}

export function passwordLogin(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions
): SyncResponse {
  const body = safeJson<LoginBody>(req);
  if (!body?.email || !body?.password)
    throw new HttpError('invalid_request', 400);
  if (body.password.length > MAX_PASSWORD_LEN)
    throw new HttpError('invalid_credentials', 401);
  const email = body.email.toLowerCase().trim();
  if (email.length === 0 || email.length > MAX_EMAIL_LEN)
    throw new HttpError('invalid_credentials', 401);
  const ipKey = clientKey(req, options.trustedProxyHeader);
  const limited = enforceRateLimits(ctx, [
    ...(ipKey
      ? [{ policy: AUTH_RATE_LIMITS.passwordLoginIp, actor: `ip:${ipKey}` }]
      : []),
    { policy: AUTH_RATE_LIMITS.passwordLoginEmail, actor: `email:${email}` },
  ]);
  if (limited) return limited;

  const stored = ctx.withTx(tx => {
    requireConfig(tx);
    const user = tx.db.authUser.email.find(email);
    return user && passwordAccount(tx, user.userId);
  });
  // Hash outside the transaction, and hash a dummy for unknown accounts so
  // the response time does not reveal which emails are registered.
  const valid = verifyPassword(
    body.password,
    stored?.passwordHash ?? DUMMY_PASSWORD_HASH
  );
  if (!stored?.passwordHash || !valid)
    throw new HttpError('invalid_credentials', 401);

  const session = ctx.withTx(tx => {
    const cfg = requireConfig(tx);
    // Reject a password that changed while it was being verified.
    if (
      passwordAccount(tx, stored.userId)?.passwordHash !== stored.passwordHash
    )
      throw new HttpError('invalid_credentials', 401);
    return issueSession(tx, cfg, stored.userId, req, options);
  });

  return jsonResponse({ userId: stored.userId, token: session.token }, 200, [
    session.cookie,
  ]);
}
