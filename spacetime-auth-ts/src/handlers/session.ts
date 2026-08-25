import type { SyncResponse, Request } from 'spacetimedb/server';
import { Timestamp } from 'spacetimedb';
import {
  type AuthHandlerCtx,
  type AuthTransactionCtx,
  shouldUseSecureCookies,
  clearCookie,
  errorResponse,
  jsonResponse,
  makeCookie,
  parseCookies,
  requireConfig,
  ConfigMissingError,
  userAgent,
} from './_helpers.ts';
import { signJwt, verifyJwt } from '../jwt.ts';
import { privateKeyFromPem, publicKeyFromPem } from '../keys.ts';
import { newSessionToken, uuidV7 } from '../crypto.ts';
import { type AuthHttpOptions, clientKey } from '../rate_limit.ts';
type StoredAuthSession = NonNullable<
  ReturnType<AuthTransactionCtx['db']['authSession']['sessionId']['find']>
>;

function readToken(req: Request, cookieName: string): string | null {
  const auth = req.headers.get('authorization');
  if (auth && auth.toLowerCase().startsWith('bearer '))
    return auth.slice(7).trim();
  const cookies = parseCookies(req.headers.get('cookie'));
  return cookies[cookieName] ?? null;
}

function nowSeconds(ctx: AuthHandlerCtx): number {
  return Number((ctx.timestamp.microsSinceUnixEpoch as bigint) / 1_000_000n);
}

function findLiveSession(
  tx: AuthTransactionCtx,
  claims: { sub?: string; jti?: string },
  nowMicros: bigint
): StoredAuthSession | null {
  if (!claims.sub || !claims.jti) return null;
  const session = tx.db.authSession.sessionId.find(claims.jti);
  if (!session) return null;
  if (session.userId !== claims.sub) return null;
  if ((session.expiresAt.microsSinceUnixEpoch as bigint) < nowMicros)
    return null;
  return session;
}

export function meHandler(ctx: AuthHandlerCtx, req: Request): SyncResponse {
  try {
    const result = ctx.withTx(tx => {
      const cfg = requireConfig(tx);
      const token = readToken(req, cfg.cookieName);
      if (!token) return { status: 401 as const };

      const pub = publicKeyFromPem(cfg.es256PublicKeyPem);
      const v = verifyJwt(pub, token, {
        issuer: cfg.issuerUrl,
        nowSeconds: nowSeconds(ctx),
      });
      if (!v.ok) return { status: 401 as const };
      if (
        !findLiveSession(
          tx,
          v.claims,
          ctx.timestamp.microsSinceUnixEpoch as bigint
        )
      ) {
        return { status: 401 as const };
      }

      const user = tx.db.authUser.userId.find(v.claims.sub);
      if (!user) return { status: 401 as const };

      return {
        status: 200 as const,
        body: {
          user: {
            userId: user.userId,
            email: user.email,
            emailVerified: user.emailVerified,
            name: user.name,
            image: user.image,
          },
          sessionExpiresAt: v.claims.exp,
        },
      };
    });

    if (result.status === 401) return errorResponse('unauthenticated', 401);
    return jsonResponse(result.body);
  } catch (e) {
    if (e instanceof ConfigMissingError)
      return errorResponse('config_missing', 500);
    throw e;
  }
}

export function refreshHandler(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions = {}
): SyncResponse {
  const nowMs = Number(ctx.timestamp.microsSinceUnixEpoch / 1000n);
  const sessionId = uuidV7(ctx.random, nowMs);
  const sessionToken = newSessionToken(ctx.random);

  try {
    const out = ctx.withTx(tx => {
      const cfg = requireConfig(tx);
      const token = readToken(req, cfg.cookieName);
      if (!token) return { status: 401 as const };

      const pub = publicKeyFromPem(cfg.es256PublicKeyPem);
      const v = verifyJwt(pub, token, {
        issuer: cfg.issuerUrl,
        nowSeconds: nowSeconds(ctx),
      });
      if (!v.ok) return { status: 401 as const };
      const existingSession = findLiveSession(
        tx,
        v.claims,
        ctx.timestamp.microsSinceUnixEpoch as bigint
      );
      if (!existingSession) return { status: 401 as const };

      const user = tx.db.authUser.userId.find(v.claims.sub);
      if (!user) return { status: 401 as const };
      tx.db.authSession.delete(existingSession);

      const ttlMicros = BigInt(cfg.sessionTtlSeconds) * 1_000_000n;
      tx.db.authSession.insert({
        sessionId,
        userId: user.userId,
        token: sessionToken,
        expiresAt: new Timestamp(
          ctx.timestamp.microsSinceUnixEpoch + ttlMicros
        ),
        ipAddress: clientKey(req, options.trustedProxyHeader),
        userAgent: userAgent(req),
        createdAt: ctx.timestamp,
      });

      return {
        status: 200 as const,
        privateKeyPem: cfg.es256PrivateKeyPem,
        keyId: cfg.keyId,
        issuerUrl: cfg.issuerUrl,
        cookieName: cfg.cookieName,
        sessionTtlSeconds: BigInt(cfg.sessionTtlSeconds),
        user: {
          userId: user.userId,
          email: user.email,
          emailVerified: user.emailVerified,
          name: user.name,
          image: user.image,
        },
      };
    });

    if (out.status === 401) return errorResponse('unauthenticated', 401);

    const nowSec = Math.floor(nowMs / 1000);
    const ttlSec = Number(out.sessionTtlSeconds);
    const priv = privateKeyFromPem(out.privateKeyPem);
    const jwt = signJwt(
      priv,
      {
        iss: out.issuerUrl,
        sub: out.user.userId,
        aud: out.issuerUrl,
        iat: nowSec,
        exp: nowSec + ttlSec,
        jti: sessionId,
      },
      out.keyId
    );

    return jsonResponse(
      { user: out.user, token: jwt, sessionExpiresAt: nowSec + ttlSec },
      200,
      {
        'set-cookie': makeCookie(out.cookieName, jwt, {
          maxAgeSeconds: ttlSec,
          secure: shouldUseSecureCookies(options.secureCookies),
        }),
      }
    );
  } catch (e) {
    if (e instanceof ConfigMissingError)
      return errorResponse('config_missing', 500);
    throw e;
  }
}

export function logoutHandler(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions = {}
): SyncResponse {
  try {
    const cookieName = ctx.withTx(tx => {
      const cfg = requireConfig(tx);
      const token = readToken(req, cfg.cookieName);
      if (token) {
        const pub = publicKeyFromPem(cfg.es256PublicKeyPem);
        const v = verifyJwt(pub, token, {
          issuer: cfg.issuerUrl,
          nowSeconds: nowSeconds(ctx),
        });
        if (v.ok && v.claims.jti) {
          const session = tx.db.authSession.sessionId.find(v.claims.jti);
          if (session) tx.db.authSession.delete(session);
        }
      }
      return cfg.cookieName;
    });
    return jsonResponse({ ok: true }, 200, {
      'set-cookie': clearCookie(cookieName, {
        secure: shouldUseSecureCookies(options.secureCookies),
      }),
    });
  } catch (e) {
    if (e instanceof ConfigMissingError)
      return errorResponse('config_missing', 500);
    throw e;
  }
}
