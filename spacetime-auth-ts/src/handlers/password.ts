import type { SyncResponse, Request } from 'spacetimedb/server';
import { Timestamp } from 'spacetimedb';
import {
  hashPassword,
  verifyPassword,
  newSessionToken,
  uuidV7,
} from '../crypto.ts';
import { signJwt } from '../jwt.ts';
import { privateKeyFromPem } from '../keys.ts';
import {
  type AuthHandlerCtx,
  shouldUseSecureCookies,
  userAgent,
  ConfigMissingError,
  errorResponse,
  jsonResponse,
  makeCookie,
  requireConfig,
  safeJson,
} from './_helpers.ts';
import {
  AUTH_RATE_LIMITS,
  type AuthHttpOptions,
  clientKey,
  enforceRateLimits,
} from '../rate_limit.ts';
import type { AuthAccount } from '../types.ts';

interface SignupBody {
  email: string;
  password: string;
  name?: string;
}

interface LoginBody {
  email: string;
  password: string;
}

const MIN_PASSWORD_LEN = 8;
const MAX_PASSWORD_LEN = 1024;
const MAX_EMAIL_LEN = 320;
const MAX_NAME_LEN = 128;

export function passwordSignupHandler(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions = {}
): SyncResponse {
  const body = safeJson<SignupBody>(req);
  if (!body?.email || !body?.password)
    return errorResponse('invalid_request', 400);
  if (body.password.length < MIN_PASSWORD_LEN)
    return errorResponse('password_too_short', 400);
  if (body.password.length > MAX_PASSWORD_LEN)
    return errorResponse('password_too_long', 400);
  const email = body.email.toLowerCase().trim();
  if (email.length === 0 || email.length > MAX_EMAIL_LEN)
    return errorResponse('invalid_email', 400);
  if (body.name !== undefined && body.name.length > MAX_NAME_LEN)
    return errorResponse('name_too_long', 400);
  const ipKey = clientKey(req, options.trustedProxyHeader);
  const limited = enforceRateLimits(ctx, req, [
    { policy: AUTH_RATE_LIMITS.passwordSignup, actor: `email:${email}` },
    ...(ipKey
      ? [{ policy: AUTH_RATE_LIMITS.passwordSignup, actor: `ip:${ipKey}` }]
      : []),
  ]);
  if (limited) return limited;

  const hash = hashPassword(ctx.random, body.password);
  const nowMs = Number(ctx.timestamp.microsSinceUnixEpoch / 1000n);
  const userId = uuidV7(ctx.random, nowMs);
  const accountId = uuidV7(ctx.random, nowMs);
  const sessionId = uuidV7(ctx.random, nowMs);
  const sessionToken = newSessionToken(ctx.random);

  let issuerUrl: string;
  let cookieName: string;
  let sessionTtlSeconds: bigint;
  let privateKeyPem: string;
  let keyId: string;

  try {
    ({ issuerUrl, cookieName, sessionTtlSeconds, privateKeyPem, keyId } =
      ctx.withTx(tx => {
        const cfg = requireConfig(tx);
        if (tx.db.authUser.email.find(email) != null)
          throw new EmailTakenError();

        tx.db.authUser.insert({
          userId,
          email,
          emailVerified: false,
          name: body.name,
          image: undefined,
          createdAt: ctx.timestamp,
          updatedAt: ctx.timestamp,
        });
        tx.db.authAccount.insert({
          accountId,
          userId,
          providerId: 'password',
          providerAccountId: email,
          passwordHash: hash,
          accessToken: undefined,
          refreshToken: undefined,
          accessTokenExpiresAt: undefined,
          createdAt: ctx.timestamp,
          updatedAt: ctx.timestamp,
        });
        const ttlMicros = BigInt(cfg.sessionTtlSeconds) * 1_000_000n;
        tx.db.authSession.insert({
          sessionId,
          userId,
          token: sessionToken,
          expiresAt: new Timestamp(
            (ctx.timestamp.microsSinceUnixEpoch as bigint) + ttlMicros
          ),
          ipAddress: clientKey(req, options.trustedProxyHeader),
          userAgent: userAgent(req),
          createdAt: ctx.timestamp,
        });

        return {
          issuerUrl: cfg.issuerUrl,
          cookieName: cfg.cookieName,
          sessionTtlSeconds: BigInt(cfg.sessionTtlSeconds),
          privateKeyPem: cfg.es256PrivateKeyPem,
          keyId: cfg.keyId,
        };
      }));
  } catch (e) {
    if (e instanceof EmailTakenError) return errorResponse('email_taken', 409);
    if (e instanceof ConfigMissingError)
      return errorResponse('config_missing', 500);
    throw e;
  }

  const nowSec = Math.floor(nowMs / 1000);
  const ttlSec = Number(sessionTtlSeconds);
  const privateKey = privateKeyFromPem(privateKeyPem);
  const jwt = signJwt(
    privateKey,
    {
      iss: issuerUrl,
      sub: userId,
      aud: issuerUrl,
      iat: nowSec,
      exp: nowSec + ttlSec,
      jti: sessionId,
    },
    keyId
  );

  return jsonResponse({ user: { userId, email }, token: jwt }, 200, {
    'set-cookie': makeCookie(cookieName, jwt, {
      maxAgeSeconds: ttlSec,
      secure: shouldUseSecureCookies(options.secureCookies),
    }),
  });
}

export function passwordLoginHandler(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions = {}
): SyncResponse {
  const body = safeJson<LoginBody>(req);
  if (!body?.email || !body?.password)
    return errorResponse('invalid_request', 400);
  if (body.password.length > MAX_PASSWORD_LEN)
    return errorResponse('invalid_credentials', 401);
  const email = body.email.toLowerCase().trim();
  if (email.length === 0 || email.length > MAX_EMAIL_LEN)
    return errorResponse('invalid_credentials', 401);
  const ipKey = clientKey(req, options.trustedProxyHeader);
  const limited = enforceRateLimits(ctx, req, [
    ...(ipKey
      ? [{ policy: AUTH_RATE_LIMITS.passwordLoginIp, actor: `ip:${ipKey}` }]
      : []),
    { policy: AUTH_RATE_LIMITS.passwordLoginEmail, actor: `email:${email}` },
  ]);
  if (limited) return limited;

  const nowMs = Number(ctx.timestamp.microsSinceUnixEpoch / 1000n);
  const sessionId = uuidV7(ctx.random, nowMs);
  const sessionToken = newSessionToken(ctx.random);

  let issuerUrl: string;
  let cookieName: string;
  let sessionTtlSeconds: bigint;
  let privateKeyPem: string;
  let keyId: string;
  let loggedInUserId: string;

  try {
    ({
      issuerUrl,
      cookieName,
      sessionTtlSeconds,
      privateKeyPem,
      keyId,
      loggedInUserId,
    } = ctx.withTx(tx => {
      const cfg = requireConfig(tx);
      const user = tx.db.authUser.email.find(email);
      if (!user) throw new InvalidCredentialsError();

      let acct: AuthAccount | undefined;
      for (const a of tx.db.authAccount.providerAccountId.filter(email)) {
        if (a.providerId === 'password' && a.userId === user.userId) {
          acct = a;
          break;
        }
      }
      if (!acct?.passwordHash) throw new InvalidCredentialsError();
      if (!verifyPassword(body.password, acct.passwordHash))
        throw new InvalidCredentialsError();

      tx.db.authSession.insert({
        sessionId,
        userId: user.userId,
        token: sessionToken,
        expiresAt: new Timestamp(
          ctx.timestamp.microsSinceUnixEpoch +
            BigInt(cfg.sessionTtlSeconds) * 1_000_000n
        ),
        ipAddress: clientKey(req, options.trustedProxyHeader),
        userAgent: userAgent(req),
        createdAt: ctx.timestamp,
      });

      return {
        issuerUrl: cfg.issuerUrl,
        cookieName: cfg.cookieName,
        sessionTtlSeconds: BigInt(cfg.sessionTtlSeconds),
        privateKeyPem: cfg.es256PrivateKeyPem,
        keyId: cfg.keyId,
        loggedInUserId: user.userId,
      };
    }));
  } catch (e) {
    if (e instanceof InvalidCredentialsError)
      return errorResponse('invalid_credentials', 401);
    if (e instanceof ConfigMissingError)
      return errorResponse('config_missing', 500);
    throw e;
  }

  const nowSec = Math.floor(nowMs / 1000);
  const ttlSec = Number(sessionTtlSeconds);
  const privateKey = privateKeyFromPem(privateKeyPem);
  const jwt = signJwt(
    privateKey,
    {
      iss: issuerUrl,
      sub: loggedInUserId,
      aud: issuerUrl,
      iat: nowSec,
      exp: nowSec + ttlSec,
      jti: sessionId,
    },
    keyId
  );

  return jsonResponse({ userId: loggedInUserId, token: jwt }, 200, {
    'set-cookie': makeCookie(cookieName, jwt, {
      maxAgeSeconds: ttlSec,
      secure: shouldUseSecureCookies(options.secureCookies),
    }),
  });
}

class EmailTakenError extends Error {
  constructor() {
    super('email_taken');
  }
}
class InvalidCredentialsError extends Error {
  constructor() {
    super('invalid_credentials');
  }
}
