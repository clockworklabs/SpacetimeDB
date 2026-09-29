import type { SyncResponse, Request } from 'spacetimedb/server';
import type { AuthHandlerCtx } from '../context';
import type { AuthHttpOptions } from '../client';
import { clearCookie, HttpError, jsonResponse, requireConfig } from './http';
import { shouldUseSecureCookies } from '../request-trust';
import { deleteSession, issueSession, requestSession } from '../sessions';
import type { AuthUser } from '../types';

function publicUser(user: AuthUser) {
  return {
    userId: user.userId,
    email: user.email,
    emailVerified: user.emailVerified,
    name: user.name,
    image: user.image,
  };
}

export function me(ctx: AuthHandlerCtx, req: Request): SyncResponse {
  return ctx.withTx(tx => {
    const session = requestSession(tx, requireConfig(tx), req);
    const user = session && tx.db.authUser.userId.find(session.userId);
    if (!user) throw new HttpError('unauthenticated', 401);
    return jsonResponse({
      user: publicUser(user),
      sessionExpiresAt: Number(
        session.expiresAt.microsSinceUnixEpoch / 1_000_000n
      ),
    });
  });
}

/** Replaces the presented session. Connections bound to it move to the new one. */
export function refresh(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions
): SyncResponse {
  return ctx.withTx(tx => {
    const cfg = requireConfig(tx);
    const old = requestSession(tx, cfg, req);
    const user = old && tx.db.authUser.userId.find(old.userId);
    if (!user) throw new HttpError('unauthenticated', 401);

    const next = issueSession(tx, cfg, user.userId, req, options);
    for (const binding of [
      ...tx.db.authConnectionBinding.sessionId.filter(old.sessionId),
    ]) {
      tx.db.authConnectionBinding.stdbIdentity.update({
        ...binding,
        sessionId: next.sessionId,
      });
    }
    tx.db.authSession.delete(old);

    return jsonResponse(
      {
        user: publicUser(user),
        token: next.token,
        sessionExpiresAt: next.expiresAt,
      },
      200,
      [next.cookie]
    );
  });
}

export function logout(
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions
): SyncResponse {
  const cookieName = ctx.withTx(tx => {
    const cfg = requireConfig(tx);
    const session = requestSession(tx, cfg, req);
    if (session) deleteSession(tx, session);
    return cfg.cookieName;
  });
  return jsonResponse({ ok: true }, 200, [
    clearCookie(cookieName, shouldUseSecureCookies(options.secureCookies)),
  ]);
}
