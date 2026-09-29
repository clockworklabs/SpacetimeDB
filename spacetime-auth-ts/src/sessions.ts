import { Timestamp } from 'spacetimedb';
import type { Request } from 'spacetimedb/server';
import type { AuthTransactionCtx } from './context';
import { makeCookie, readBearer } from './handlers/http';
import { signJwt, verifyJwt } from './jwt';
import { privateKeyFromPem, publicKeyFromPem } from './keys';
import { clientKey, shouldUseSecureCookies, userAgent } from './request-trust';
import { newToken } from './tokens';
import type { AuthConfig, AuthSession } from './types';
import type { AuthHttpOptions } from './client';

/** Deletes a session and every connection bound to it. */
export function deleteSession(tx: AuthTransactionCtx, session: AuthSession) {
  for (const binding of [
    ...tx.db.authConnectionBinding.sessionId.filter(session.sessionId),
  ]) {
    tx.db.authConnectionBinding.delete(binding);
  }
  tx.db.authSession.delete(session);
}

export function isLive(session: AuthSession, now: Timestamp): boolean {
  return session.expiresAt.microsSinceUnixEpoch > now.microsSinceUnixEpoch;
}

/** Inserts a session and returns its signed token and cookie. */
export function issueSession(
  tx: AuthTransactionCtx,
  cfg: AuthConfig,
  userId: string,
  req: Request,
  options: AuthHttpOptions
): { sessionId: string; token: string; expiresAt: number; cookie: string } {
  const sessionId = newToken(tx);
  const nowSec = Number(tx.timestamp.microsSinceUnixEpoch / 1_000_000n);
  const ttlSec = Number(cfg.sessionTtlSeconds);
  tx.db.authSession.insert({
    sessionId,
    userId,
    expiresAt: new Timestamp(
      tx.timestamp.microsSinceUnixEpoch + cfg.sessionTtlSeconds * 1_000_000n
    ),
    ipAddress: clientKey(req, options.trustedProxyHeader),
    userAgent: userAgent(req),
    createdAt: tx.timestamp,
  });
  const token = signJwt(
    privateKeyFromPem(cfg.es256PrivateKeyPem),
    {
      iss: cfg.issuerUrl,
      sub: userId,
      aud: cfg.issuerUrl,
      iat: nowSec,
      exp: nowSec + ttlSec,
      jti: sessionId,
    },
    cfg.keyId
  );
  return {
    sessionId,
    token,
    expiresAt: nowSec + ttlSec,
    cookie: makeCookie(cfg.cookieName, token, {
      maxAgeSeconds: ttlSec,
      secure: shouldUseSecureCookies(options.secureCookies),
    }),
  };
}

/** The live session for a signed session token, or undefined. */
export function tokenSession(
  tx: AuthTransactionCtx,
  cfg: AuthConfig,
  token: string
): AuthSession | undefined {
  const v = verifyJwt(publicKeyFromPem(cfg.es256PublicKeyPem), token, {
    issuer: cfg.issuerUrl,
    nowSeconds: Number(tx.timestamp.microsSinceUnixEpoch / 1_000_000n),
  });
  if (!v.ok || !v.claims.jti) return undefined;
  const session = tx.db.authSession.sessionId.find(v.claims.jti);
  if (!session || session.userId !== v.claims.sub) return undefined;
  return isLive(session, tx.timestamp) ? session : undefined;
}

/** The live session presented by a request's bearer token or session cookie. */
export function requestSession(
  tx: AuthTransactionCtx,
  cfg: AuthConfig,
  req: Request
): AuthSession | undefined {
  const token = readBearer(req, cfg.cookieName);
  return token ? tokenSession(tx, cfg, token) : undefined;
}

/**
 * The user id of the live session presented by a request's bearer token or
 * session cookie. Use it in HTTP handlers, which have no caller identity.
 */
export function requestUserId(
  tx: AuthTransactionCtx,
  req: Request
): string | undefined {
  const cfg = tx.db.authConfig.singleton.find(true);
  return cfg ? requestSession(tx, cfg, req)?.userId : undefined;
}
