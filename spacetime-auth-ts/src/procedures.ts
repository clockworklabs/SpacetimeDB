import type { Timestamp } from 'spacetimedb';
import {
  Range,
  t,
  SenderError,
  type InferTypeOfParams,
} from 'spacetimedb/server';
import {
  generateEs256Keypair,
  fromPrivateKeyBytes,
  privateKeyFromPem,
  publicKeyFromPem,
} from './keys.ts';
import { verifyJwt } from './jwt.ts';
import { authAdminVerdict, denyIfNotAdmin } from './admin.ts';
import type {
  AuthProcedureCtx,
  AuthReducerCtx,
  AuthTransactionCtx,
} from './context.ts';

type AuthWriteCtx = AuthReducerCtx | AuthProcedureCtx;

function withCtx<T>(ctx: AuthWriteCtx, fn: (tx: AuthTransactionCtx) => T): T {
  return 'withTx' in ctx ? ctx.withTx(fn) : fn(ctx);
}

export const setAuthConfigParams = {
  issuerUrl: t.string(),
  baseUrl: t.option(t.string()),
  cookieName: t.option(t.string()),
  sessionTtlSeconds: t.option(t.u64()),
  /** If omitted on first call, a fresh keypair is generated. */
  es256PrivateKeyPem: t.option(t.string()),
  googleClientId: t.option(t.string()),
  googleClientSecret: t.option(t.string()),
  githubClientId: t.option(t.string()),
  githubClientSecret: t.option(t.string()),
};

const DEFAULT_COOKIE_NAME = 'stdb_auth';
const DEFAULT_SESSION_TTL_SECONDS = 60n * 60n * 24n * 7n;

export function setAuthConfigImpl(
  ctx: AuthWriteCtx,
  args: InferTypeOfParams<typeof setAuthConfigParams>
): void {
  // Requires an admin row seeded by the database owner. Without this, any
  // client could rotate the signing keys / overwrite OAuth secrets and forge
  // sessions. The verdict is read inside a tx but the denial is thrown outside
  // it (a SenderError thrown inside ctx.withTx surfaces as a fatal instance
  // error, not a clean rejection).
  const verdict = withCtx(ctx, tx => authAdminVerdict(tx, ctx.sender));
  denyIfNotAdmin(verdict);

  withCtx(ctx, tx => {
    const existing = tx.db.authConfig.singleton.find(true);

    let privateKeyPem: string;
    let publicKeyPem: string;
    let keyId: string;

    if (args.es256PrivateKeyPem) {
      let raw: Uint8Array;
      try {
        raw = privateKeyFromPem(args.es256PrivateKeyPem);
      } catch (e) {
        throw new SenderError(
          `auth.invalid_private_key_pem:${(e as Error).message}`
        );
      }
      const kp = fromPrivateKeyBytes(raw);
      privateKeyPem = kp.privateKeyPem;
      publicKeyPem = kp.publicKeyPem;
      keyId = kp.kid;
    } else if (existing) {
      privateKeyPem = existing.es256PrivateKeyPem;
      publicKeyPem = existing.es256PublicKeyPem;
      keyId = existing.keyId;
    } else {
      const kp = generateEs256Keypair(ctx.random);
      privateKeyPem = kp.privateKeyPem;
      publicKeyPem = kp.publicKeyPem;
      keyId = kp.kid;
    }

    if (existing) {
      tx.db.authConfig.singleton.update({
        ...existing,
        issuerUrl: args.issuerUrl,
        baseUrl: args.baseUrl ?? existing.baseUrl,
        cookieName: args.cookieName ?? existing.cookieName,
        sessionTtlSeconds: args.sessionTtlSeconds ?? existing.sessionTtlSeconds,
        es256PrivateKeyPem: privateKeyPem,
        es256PublicKeyPem: publicKeyPem,
        keyId,
        googleClientId: args.googleClientId ?? existing.googleClientId,
        googleClientSecret:
          args.googleClientSecret ?? existing.googleClientSecret,
        githubClientId: args.githubClientId ?? existing.githubClientId,
        githubClientSecret:
          args.githubClientSecret ?? existing.githubClientSecret,
        updatedAt: ctx.timestamp,
      });
      return;
    }

    tx.db.authConfig.insert({
      singleton: true,
      issuerUrl: args.issuerUrl,
      baseUrl: args.baseUrl ?? args.issuerUrl,
      cookieName: args.cookieName ?? DEFAULT_COOKIE_NAME,
      sessionTtlSeconds: args.sessionTtlSeconds ?? DEFAULT_SESSION_TTL_SECONDS,
      es256PrivateKeyPem: privateKeyPem,
      es256PublicKeyPem: publicKeyPem,
      keyId,
      googleClientId: args.googleClientId,
      googleClientSecret: args.googleClientSecret,
      githubClientId: args.githubClientId,
      githubClientSecret: args.githubClientSecret,
      updatedAt: ctx.timestamp,
    });
  });
}

const SWEEP_BATCH = 500;

export function authSweepImpl(ctx: AuthWriteCtx): void {
  const nowMicros = ctx.timestamp.microsSinceUnixEpoch as bigint;
  withCtx(ctx, tx => {
    let n = 0;
    for (const row of tx.db.authSession.expiresAt.filter(
      new Range(undefined, { tag: 'excluded', value: ctx.timestamp })
    )) {
      if (n >= SWEEP_BATCH) break;
      if ((row.expiresAt.microsSinceUnixEpoch as bigint) < nowMicros) {
        tx.db.authSession.delete(row);
        n++;
      }
    }
    for (const row of tx.db.authVerification.expiresAt.filter(
      new Range(undefined, { tag: 'excluded', value: ctx.timestamp })
    )) {
      if (n >= SWEEP_BATCH) break;
      if ((row.expiresAt.microsSinceUnixEpoch as bigint) < nowMicros) {
        tx.db.authVerification.delete(row);
        n++;
      }
    }
    for (const row of tx.db.authOauthState.expiresAt.filter(
      new Range(undefined, { tag: 'excluded', value: ctx.timestamp })
    )) {
      if (n >= SWEEP_BATCH) break;
      if ((row.expiresAt.microsSinceUnixEpoch as bigint) < nowMicros) {
        tx.db.authOauthState.delete(row);
        n++;
      }
    }
  });
}

export const revokeSessionParams = { sessionId: t.string() };

export function revokeSessionImpl(
  ctx: AuthWriteCtx,
  args: InferTypeOfParams<typeof revokeSessionParams>
): void {
  // Admin action for revoking any user's session. Self-service revocation is
  // revokeMySessionImpl (caller-scoped). Verdict inside tx, deny outside.
  const verdict = withCtx(ctx, tx => authAdminVerdict(tx, ctx.sender));
  denyIfNotAdmin(verdict);

  withCtx(ctx, tx => {
    const s = tx.db.authSession.sessionId.find(args.sessionId);
    if (s) tx.db.authSession.delete(s);
  });
}

export const listMySessionsParams = {};

export interface MySessionSummary {
  sessionId: string;
  expiresAt: Timestamp;
  createdAt: Timestamp;
  ipAddress: string | undefined;
  userAgent: string | undefined;
  isCurrent: boolean;
}

export function listMySessionsImpl(
  ctx: AuthWriteCtx,
  _args: Record<never, never>
): { sessions: MySessionSummary[] } {
  return withCtx(ctx, tx => {
    const binding = tx.db.authConnectionBinding.stdbIdentity.find(ctx.sender);
    if (!binding) return { sessions: [] };
    const userId = binding.userId;
    const nowMicros = ctx.timestamp.microsSinceUnixEpoch as bigint;
    const sessions: MySessionSummary[] = [];
    for (const s of tx.db.authSession.userId.filter(userId)) {
      if ((s.expiresAt.microsSinceUnixEpoch as bigint) < nowMicros) continue;
      sessions.push({
        sessionId: s.sessionId,
        expiresAt: s.expiresAt,
        createdAt: s.createdAt,
        ipAddress: s.ipAddress,
        userAgent: s.userAgent,
        isCurrent: false,
      });
    }
    sessions.sort((a, b) =>
      Number(
        (b.createdAt.microsSinceUnixEpoch as bigint) -
          (a.createdAt.microsSinceUnixEpoch as bigint)
      )
    );
    return { sessions };
  });
}

export const revokeMySessionParams = { sessionId: t.string() };

export function revokeMySessionImpl(
  ctx: AuthWriteCtx,
  args: InferTypeOfParams<typeof revokeMySessionParams>
): void {
  withCtx(ctx, tx => {
    const binding = tx.db.authConnectionBinding.stdbIdentity.find(ctx.sender);
    if (!binding) throw new SenderError('auth.not_authenticated');
    const s = tx.db.authSession.sessionId.find(args.sessionId);
    if (!s) return;
    if (s.userId !== binding.userId)
      throw new SenderError('auth.session_not_owned');
    tx.db.authSession.delete(s);
  });
}

export const getPublicKeyPemParams = {};

export function getPublicKeyPemImpl(
  ctx: AuthWriteCtx,
  _args: Record<never, never>
): { publicKeyPem: string; keyId: string; issuerUrl: string } {
  return withCtx(ctx, tx => {
    const cfg = tx.db.authConfig.singleton.find(true);
    if (!cfg) throw new SenderError('auth.config_missing');
    return {
      publicKeyPem: cfg.es256PublicKeyPem,
      keyId: cfg.keyId,
      issuerUrl: cfg.issuerUrl,
    };
  });
}

/** Call once after each STDB connect. Idempotent. */
export const linkConnectionParams = { sessionToken: t.string() };

const RETRY_FAILED_MSG = 'transaction retry failed again';

export function linkConnectionImpl(
  ctx: AuthWriteCtx,
  args: InferTypeOfParams<typeof linkConnectionParams>
): { userId: string } {
  try {
    return withCtx(ctx, tx => {
      const cfg = tx.db.authConfig.singleton.find(true);
      if (!cfg) throw new SenderError('auth.config_missing');

      const pub = publicKeyFromPem(cfg.es256PublicKeyPem);
      const nowMicros = ctx.timestamp.microsSinceUnixEpoch as bigint;
      const nowSec = Number(nowMicros / 1_000_000n);
      const v = verifyJwt(pub, args.sessionToken, {
        issuer: cfg.issuerUrl,
        nowSeconds: nowSec,
      });
      if (!v.ok) throw new SenderError(`auth.invalid_token:${v.reason}`);

      const userId = v.claims.sub;
      if (!userId) throw new SenderError('auth.token_missing_sub');
      const sessionId = v.claims.jti;
      if (!sessionId) throw new SenderError('auth.token_missing_session');

      const session = tx.db.authSession.sessionId.find(sessionId);
      if (!session || session.userId !== userId)
        throw new SenderError('auth.session_not_found');
      if ((session.expiresAt.microsSinceUnixEpoch as bigint) < nowMicros) {
        throw new SenderError('auth.session_expired');
      }

      const existing = tx.db.authConnectionBinding.stdbIdentity.find(
        ctx.sender
      );
      if (existing) {
        tx.db.authConnectionBinding.stdbIdentity.update({
          ...existing,
          userId,
          linkedAt: ctx.timestamp,
        });
      } else {
        tx.db.authConnectionBinding.insert({
          stdbIdentity: ctx.sender,
          userId,
          linkedAt: ctx.timestamp,
        });
      }
      return { userId };
    });
  } catch (e: unknown) {
    if (e instanceof Error && e.message.includes(RETRY_FAILED_MSG)) {
      throw new SenderError('auth.link_busy_retry');
    }
    throw e;
  }
}

export const unlinkConnectionParams = {};

export function unlinkConnectionImpl(
  ctx: AuthWriteCtx,
  _args: Record<never, never>
): void {
  withCtx(ctx, tx => {
    const existing = tx.db.authConnectionBinding.stdbIdentity.find(ctx.sender);
    if (existing) tx.db.authConnectionBinding.delete(existing);
  });
}

const MAX_NAME_LEN = 64;
const MAX_IMAGE_LEN = 2048;

/** Caller updates their own display name / image. Either field, when present,
 * sets the row's value; pass an empty string to clear it (becomes none). */
export const updateProfileParams = {
  name: t.option(t.string()),
  image: t.option(t.string()),
};

export function updateProfileImpl(
  ctx: AuthWriteCtx,
  args: InferTypeOfParams<typeof updateProfileParams>
): void {
  withCtx(ctx, tx => {
    const binding = tx.db.authConnectionBinding.stdbIdentity.find(ctx.sender);
    if (!binding) throw new SenderError('auth.not_authenticated');
    const user = tx.db.authUser.userId.find(binding.userId);
    if (!user) throw new SenderError('auth.user_not_found');

    const nextName =
      args.name === undefined
        ? user.name
        : args.name.length === 0
          ? undefined
          : args.name.trim();
    const nextImage =
      args.image === undefined
        ? user.image
        : args.image.length === 0
          ? undefined
          : args.image.trim();
    if (nextName !== undefined && nextName.length > MAX_NAME_LEN) {
      throw new SenderError(`auth.name_too_long:max=${MAX_NAME_LEN}`);
    }
    if (nextImage !== undefined && nextImage.length > MAX_IMAGE_LEN) {
      throw new SenderError(`auth.image_too_long:max=${MAX_IMAGE_LEN}`);
    }

    tx.db.authUser.userId.update({
      ...user,
      name: nextName,
      image: nextImage,
      updatedAt: ctx.timestamp,
    });
  });
}
