import {
  Range,
  SenderError,
  t,
  type Infer,
  type InferTypeOfParams,
} from 'spacetimedb/server';
import type { Identity } from 'spacetimedb';
import { fromPrivateKeyBytes, privateKeyFromPem } from './keys.js';
import type { AuthProcedureCtx, AuthReducerCtx } from './context.js';
import { requireCallerUserId } from './caller.js';
import { errors } from './errors.js';
import { deleteSession, isLive, tokenSession } from './sessions.js';

function requireAdmin(ctx: AuthReducerCtx): void {
  if (ctx.db.authAdminIdentity.identity.find(ctx.sender) == null)
    throw new SenderError(errors.notAuthorized);
}

export const setAuthConfigParams = {
  issuerUrl: t.string(),
  baseUrl: t.option(t.string()),
  cookieName: t.option(t.string()),
  sessionTtlSeconds: t.option(t.u64()),
  /** Required on the first call. Generate it outside the module. */
  es256PrivateKeyPem: t.option(t.string()),
  googleClientId: t.option(t.string()),
  googleClientSecret: t.option(t.string()),
  githubClientId: t.option(t.string()),
  githubClientSecret: t.option(t.string()),
};

const DEFAULT_COOKIE_NAME = 'stdb_auth';
const DEFAULT_SESSION_TTL_SECONDS = 60n * 60n * 24n * 7n;

// Admin only: the signing key and OAuth secrets protect every session.
export function setAuthConfig(
  ctx: AuthReducerCtx,
  args: InferTypeOfParams<typeof setAuthConfigParams>
): void {
  requireAdmin(ctx);
  const existing = ctx.db.authConfig.singleton.find(true);

  let key = existing && {
    privateKeyPem: existing.es256PrivateKeyPem,
    publicKeyPem: existing.es256PublicKeyPem,
    kid: existing.keyId,
  };
  if (args.es256PrivateKeyPem) {
    try {
      key = fromPrivateKeyBytes(privateKeyFromPem(args.es256PrivateKeyPem));
    } catch (e) {
      throw new SenderError(
        `${errors.invalidPrivateKeyPem}:${(e as Error).message}`
      );
    }
  }
  if (!key) throw new SenderError(errors.signingKeyRequired);

  const row = {
    singleton: true,
    issuerUrl: args.issuerUrl,
    baseUrl: args.baseUrl ?? existing?.baseUrl ?? args.issuerUrl,
    cookieName: args.cookieName ?? existing?.cookieName ?? DEFAULT_COOKIE_NAME,
    sessionTtlSeconds:
      args.sessionTtlSeconds ??
      existing?.sessionTtlSeconds ??
      DEFAULT_SESSION_TTL_SECONDS,
    es256PrivateKeyPem: key.privateKeyPem,
    es256PublicKeyPem: key.publicKeyPem,
    keyId: key.kid,
    tokenCounter: existing?.tokenCounter ?? 0n,
    googleClientId: args.googleClientId ?? existing?.googleClientId,
    googleClientSecret: args.googleClientSecret ?? existing?.googleClientSecret,
    githubClientId: args.githubClientId ?? existing?.githubClientId,
    githubClientSecret: args.githubClientSecret ?? existing?.githubClientSecret,
    updatedAt: ctx.timestamp,
  };
  if (existing) ctx.db.authConfig.singleton.update(row);
  else ctx.db.authConfig.insert(row);
}

export const adminParams = { identity: t.identity() };

export function addAuthAdmin(
  ctx: AuthReducerCtx,
  { identity }: { identity: Identity }
): void {
  requireAdmin(ctx);
  if (ctx.db.authAdminIdentity.identity.find(identity) == null) {
    ctx.db.authAdminIdentity.insert({
      identity,
      addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
}

export function removeAuthAdmin(
  ctx: AuthReducerCtx,
  { identity }: { identity: Identity }
): void {
  requireAdmin(ctx);
  if (ctx.db.authAdminIdentity.identity.find(identity) == null) return;
  if (ctx.db.authAdminIdentity.count() <= 1n)
    throw new SenderError(errors.lastAdmin);
  ctx.db.authAdminIdentity.identity.delete(identity);
}

/** Deletes every expired session, verification, and OAuth state. */
export function authSweep(ctx: AuthReducerCtx): void {
  const expired = new Range(undefined, {
    tag: 'included' as const,
    value: ctx.timestamp,
  });
  for (const row of [...ctx.db.authSession.expiresAt.filter(expired)]) {
    deleteSession(ctx, row);
  }
  for (const row of [...ctx.db.authVerification.expiresAt.filter(expired)]) {
    ctx.db.authVerification.delete(row);
  }
  for (const row of [...ctx.db.authOauthState.expiresAt.filter(expired)]) {
    ctx.db.authOauthState.delete(row);
  }
}

export const sessionIdParams = { sessionId: t.string() };

/** Admin action for revoking any user's session. */
export function revokeSession(
  ctx: AuthReducerCtx,
  args: { sessionId: string }
): void {
  requireAdmin(ctx);
  const s = ctx.db.authSession.sessionId.find(args.sessionId);
  if (s) deleteSession(ctx, s);
}

export function revokeMySession(
  ctx: AuthReducerCtx,
  args: { sessionId: string }
): void {
  const userId = requireCallerUserId(ctx);
  const s = ctx.db.authSession.sessionId.find(args.sessionId);
  if (!s) return;
  if (s.userId !== userId) throw new SenderError(errors.sessionNotOwned);
  deleteSession(ctx, s);
}

export const mySessions = t.object('MySessions', {
  sessions: t.array(
    t.object('MySession', {
      sessionId: t.string(),
      expiresAt: t.timestamp(),
      createdAt: t.timestamp(),
      ipAddress: t.option(t.string()),
      userAgent: t.option(t.string()),
      isCurrent: t.bool(),
    })
  ),
});

export function listMySessions(
  ctx: AuthProcedureCtx
): Infer<typeof mySessions> {
  return ctx.withTx(tx => {
    const binding = tx.db.authConnectionBinding.stdbIdentity.find(ctx.sender);
    const current =
      binding && tx.db.authSession.sessionId.find(binding.sessionId);
    if (!binding || !current || !isLive(current, tx.timestamp))
      return { sessions: [] };
    const sessions = [...tx.db.authSession.userId.filter(binding.userId)]
      .filter(s => isLive(s, tx.timestamp))
      .map(s => ({
        sessionId: s.sessionId,
        expiresAt: s.expiresAt,
        createdAt: s.createdAt,
        ipAddress: s.ipAddress,
        userAgent: s.userAgent,
        isCurrent: s.sessionId === binding.sessionId,
      }));
    sessions.sort((a, b) =>
      Number(
        b.createdAt.microsSinceUnixEpoch - a.createdAt.microsSinceUnixEpoch
      )
    );
    return { sessions };
  });
}

export const authPublicKey = t.object('AuthPubKey', {
  publicKeyPem: t.string(),
  keyId: t.string(),
  issuerUrl: t.string(),
});

export function getAuthPublicKey(
  ctx: AuthProcedureCtx
): Infer<typeof authPublicKey> {
  return ctx.withTx(tx => {
    const cfg = tx.db.authConfig.singleton.find(true);
    if (!cfg) throw new SenderError(errors.configMissing);
    return {
      publicKeyPem: cfg.es256PublicKeyPem,
      keyId: cfg.keyId,
      issuerUrl: cfg.issuerUrl,
    };
  });
}

/** Call once after each STDB connect. Idempotent. */
export const linkConnectionParams = { sessionToken: t.string() };

export function linkConnection(
  ctx: AuthReducerCtx,
  args: { sessionToken: string }
): void {
  const cfg = ctx.db.authConfig.singleton.find(true);
  if (!cfg) throw new SenderError(errors.configMissing);
  const session = tokenSession(ctx, cfg, args.sessionToken);
  if (!session) throw new SenderError(errors.invalidToken);

  const binding = {
    stdbIdentity: ctx.sender,
    userId: session.userId,
    sessionId: session.sessionId,
    linkedAt: ctx.timestamp,
  };
  if (ctx.db.authConnectionBinding.stdbIdentity.find(ctx.sender)) {
    ctx.db.authConnectionBinding.stdbIdentity.update(binding);
  } else {
    ctx.db.authConnectionBinding.insert(binding);
  }
}

export function unlinkConnection(ctx: AuthReducerCtx): void {
  ctx.db.authConnectionBinding.stdbIdentity.delete(ctx.sender);
}

const MAX_NAME_LEN = 64;
const MAX_IMAGE_LEN = 2048;

/** Caller updates their own display name / image. Either field, when present,
 * sets the row's value; pass an empty string to clear it (becomes none). */
export const updateProfileParams = {
  name: t.option(t.string()),
  image: t.option(t.string()),
};

export function updateProfile(
  ctx: AuthReducerCtx,
  args: InferTypeOfParams<typeof updateProfileParams>
): void {
  const userId = requireCallerUserId(ctx);
  const user = ctx.db.authUser.userId.find(userId);
  if (!user) throw new SenderError(errors.userNotFound);

  const next = (value: string | undefined, current: string | undefined) =>
    value === undefined ? current : value.trim() || undefined;
  const name = next(args.name, user.name);
  const image = next(args.image, user.image);
  if (name !== undefined && name.length > MAX_NAME_LEN) {
    throw new SenderError(`${errors.nameTooLong}:max=${MAX_NAME_LEN}`);
  }
  if (image !== undefined && image.length > MAX_IMAGE_LEN) {
    throw new SenderError(`${errors.imageTooLong}:max=${MAX_IMAGE_LEN}`);
  }

  ctx.db.authUser.userId.update({
    ...user,
    name,
    image,
    updatedAt: ctx.timestamp,
  });
}
