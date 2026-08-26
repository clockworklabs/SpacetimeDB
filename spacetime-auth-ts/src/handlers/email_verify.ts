import type { SyncResponse, Request } from 'spacetimedb/server';
import { uuidV7, randomToken } from '../crypto.ts';
import {
  buildVerifyEmail,
  MailerNotConfiguredError,
  type SendMailFn,
} from '../mailer.ts';
import {
  type AuthHandlerCtx,
  ConfigMissingError,
  errorResponse,
  jsonResponse,
  parseCookies,
  parseQueryString,
  redirectResponse,
  requireConfig,
} from './http.ts';
import { verifyJwt } from '../jwt.ts';
import { publicKeyFromPem } from '../keys.ts';
import { Timestamp } from 'spacetimedb';
import {
  AUTH_RATE_LIMITS,
  type AuthHttpOptions,
  enforceIpRateLimit,
} from '../rate_limit.ts';

const PURPOSE = 'email_verify';
const TOKEN_TTL_SECONDS = 60n * 60n * 24n;

export interface VerifyRequestOpts extends AuthHttpOptions {
  sendMail: SendMailFn;
  appName?: string;
  /** Default '/'. */
  successRedirect?: string;
}

export function makeEmailVerifyRequestHandler(opts: VerifyRequestOpts) {
  return function emailVerifyRequest(
    ctx: AuthHandlerCtx,
    req: Request
  ): SyncResponse {
    if (!opts.sendMail) throw new MailerNotConfiguredError();
    const limited = enforceIpRateLimit(
      ctx,
      req,
      AUTH_RATE_LIMITS.emailVerifyRequest,
      opts.trustedProxyHeader
    );
    if (limited) return limited;

    const verificationId = uuidV7(
      ctx.random,
      Number(ctx.timestamp.microsSinceUnixEpoch / 1000n)
    );
    const token = randomToken(ctx.random, 32);

    let userEmail: string;
    let baseUrl: string;
    try {
      ({ userEmail, baseUrl } = ctx.withTx(tx => {
        const cfg = requireConfig(tx);
        const cookies = parseCookies(req.headers.get('cookie'));
        const bearer = req.headers.get('authorization');
        const sessionToken =
          bearer && bearer.toLowerCase().startsWith('bearer ')
            ? bearer.slice(7).trim()
            : cookies[cfg.cookieName];
        if (!sessionToken) throw new UnauthenticatedError();

        const pub = publicKeyFromPem(cfg.es256PublicKeyPem);
        const nowMicros = ctx.timestamp.microsSinceUnixEpoch as bigint;
        const v = verifyJwt(pub, sessionToken, {
          issuer: cfg.issuerUrl,
          nowSeconds: Number(nowMicros / 1_000_000n),
        });
        if (!v.ok) throw new UnauthenticatedError();
        if (!v.claims.jti) throw new UnauthenticatedError();

        const session = tx.db.authSession.sessionId.find(v.claims.jti);
        if (!session || session.userId !== v.claims.sub)
          throw new UnauthenticatedError();
        if ((session.expiresAt.microsSinceUnixEpoch as bigint) < nowMicros) {
          throw new UnauthenticatedError();
        }

        const user = tx.db.authUser.userId.find(v.claims.sub);
        if (!user) throw new UnauthenticatedError();
        if (user.emailVerified) throw new AlreadyVerifiedError();

        for (const row of tx.db.authVerification.identifier.filter(
          user.email
        )) {
          if (row.purpose === PURPOSE) tx.db.authVerification.delete(row);
        }

        tx.db.authVerification.insert({
          verificationId,
          identifier: user.email,
          value: token,
          purpose: PURPOSE,
          expiresAt: new Timestamp(
            ctx.timestamp.microsSinceUnixEpoch + TOKEN_TTL_SECONDS * 1_000_000n
          ),
          createdAt: ctx.timestamp,
        });
        return { userEmail: user.email, baseUrl: cfg.baseUrl };
      }));
    } catch (e) {
      if (e instanceof UnauthenticatedError)
        return errorResponse('unauthenticated', 401);
      if (e instanceof AlreadyVerifiedError)
        return jsonResponse({ ok: true, alreadyVerified: true });
      if (e instanceof ConfigMissingError)
        return errorResponse('config_missing', 500);
      throw e;
    }

    const mail = buildVerifyEmail({ baseUrl, token, appName: opts.appName });
    mail.to = userEmail;
    opts.sendMail(ctx, mail);
    return jsonResponse({ ok: true });
  };
}

export function makeEmailVerifyHandler(
  opts: { successRedirect?: string } = {}
) {
  return function emailVerify(ctx: AuthHandlerCtx, req: Request): SyncResponse {
    const q = parseQueryString(req.uri);
    const token = q['token'];
    if (!token) return errorResponse('missing_token', 400);

    try {
      ctx.withTx(tx => {
        const row = tx.db.authVerification.value.find(token);
        if (!row || row.purpose !== PURPOSE) throw new BadTokenError();
        if (
          row.expiresAt.microsSinceUnixEpoch <
          ctx.timestamp.microsSinceUnixEpoch
        ) {
          tx.db.authVerification.delete(row);
          throw new BadTokenError();
        }

        const user = tx.db.authUser.email.find(row.identifier);
        tx.db.authVerification.delete(row);
        if (!user) throw new BadTokenError();

        tx.db.authUser.userId.update({
          ...user,
          emailVerified: true,
          updatedAt: ctx.timestamp,
        });
      });
    } catch (e) {
      if (e instanceof BadTokenError) return errorResponse('bad_token', 400);
      throw e;
    }

    return redirectResponse(opts.successRedirect ?? '/');
  };
}

class UnauthenticatedError extends Error {
  constructor() {
    super('unauthenticated');
  }
}
class AlreadyVerifiedError extends Error {
  constructor() {
    super('already_verified');
  }
}
class BadTokenError extends Error {
  constructor() {
    super('bad_token');
  }
}
