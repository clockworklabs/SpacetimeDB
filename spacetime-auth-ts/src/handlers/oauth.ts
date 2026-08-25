import { SyncResponse, type Request } from 'spacetimedb/server';
import { Timestamp } from 'spacetimedb';
import {
  newSessionToken,
  newPkceVerifier,
  pkceChallenge,
  randomToken,
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
  makeCookie,
  parseQueryString,
  redirectResponse,
  requireConfig,
} from './_helpers.ts';
import {
  AUTH_RATE_LIMITS,
  type AuthHttpOptions,
  clientKey,
  enforceRateLimits,
} from '../rate_limit.ts';
import { safeRedirectPath } from '../request-trust.ts';
import type { AuthAccount, AuthConfig } from '../types.ts';

const OAUTH_STATE_TTL_SECONDS = 600n;
const MAX_OAUTH_CODE_LENGTH = 4096;
const MAX_OAUTH_STATE_LENGTH = 256;
const MAX_PROFILE_SUB_LENGTH = 512;
const MAX_PROFILE_EMAIL_LENGTH = 320;
const MAX_PROFILE_NAME_LENGTH = 256;
const MAX_PROFILE_IMAGE_LENGTH = 2048;

export interface OAuthProviderSpec {
  id: string;
  authorizeUrl: string;
  tokenUrl: string;
  scope: string;
  getClientId: (cfg: AuthConfig) => string;
  getClientSecret: (cfg: AuthConfig) => string;
  /** Reserved for verified OIDC id_token support. Prefer userInfoUrl. */
  oidc: boolean;
  userInfoUrl?: string;
  userInfoHeaders?: Record<string, string>;
  parseProfile: (data: unknown) => OAuthProfile;
  resolveProfile?: (
    ctx: AuthHandlerCtx,
    accessToken: string
  ) => OAuthProfile | OAuthProfileError;
  authorizeExtras?: Record<string, string>;
  /** Default true. */
  usePkce?: boolean;
}

export interface OAuthProfile {
  sub: string;
  email: string;
  emailVerified?: boolean;
  name?: string;
  image?: string;
}

export interface OAuthProfileError {
  error: string;
}

function isProfileError(
  value: OAuthProfile | OAuthProfileError
): value is OAuthProfileError {
  return typeof (value as OAuthProfileError).error === 'string';
}

export function makeOAuthStartHandler(
  provider: OAuthProviderSpec,
  defaultOptions: AuthHttpOptions = {}
) {
  return function start(
    ctx: AuthHandlerCtx,
    req: Request,
    options: AuthHttpOptions = defaultOptions
  ): SyncResponse {
    const q = parseQueryString(req.uri);
    const requestedRedirect = q['redirectTo'];
    const redirectTo =
      requestedRedirect === undefined
        ? '/'
        : safeRedirectPath(requestedRedirect);
    if (redirectTo === undefined) return errorResponse('invalid_redirect', 400);
    const ipKey = clientKey(req, options.trustedProxyHeader);
    const limited = ipKey
      ? enforceRateLimits(ctx, req, [
          {
            policy: AUTH_RATE_LIMITS.oauthStart,
            actor: `ip:${ipKey}:${provider.id}`,
          },
        ])
      : null;
    if (limited) return limited;

    const state = randomToken(ctx.random, 32);
    const verifier =
      (provider.usePkce ?? true) ? newPkceVerifier(ctx.random) : '';

    let baseUrl: string;
    let clientId: string;
    try {
      ({ baseUrl, clientId } = ctx.withTx(tx => {
        const cfg = requireConfig(tx);
        const cid = provider.getClientId(cfg);
        if (!cid) throw new ProviderNotConfiguredError(provider.id);
        tx.db.authOauthState.insert({
          state,
          provider: provider.id,
          codeVerifier: verifier,
          redirectTo,
          expiresAt: new Timestamp(
            ctx.timestamp.microsSinceUnixEpoch +
              OAUTH_STATE_TTL_SECONDS * 1_000_000n
          ),
          createdAt: ctx.timestamp,
        });
        return { baseUrl: cfg.baseUrl, clientId: cid };
      }));
    } catch (e) {
      if (e instanceof ConfigMissingError)
        return errorResponse('config_missing', 500);
      if (e instanceof ProviderNotConfiguredError)
        return errorResponse(`provider_not_configured:${e.provider}`, 500);
      throw e;
    }

    const params: Record<string, string> = {
      client_id: clientId,
      redirect_uri: `${baseUrl}/auth/${provider.id}/callback`,
      response_type: 'code',
      scope: provider.scope,
      state,
    };
    if (provider.usePkce ?? true) {
      params['code_challenge'] = pkceChallenge(verifier);
      params['code_challenge_method'] = 'S256';
    }
    for (const [k, v] of Object.entries(provider.authorizeExtras ?? {})) {
      params[k] = v;
    }
    const qs = Object.entries(params)
      .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
      .join('&');
    const sep = provider.authorizeUrl.includes('?') ? '&' : '?';
    return redirectResponse(`${provider.authorizeUrl}${sep}${qs}`);
  };
}

export function makeOAuthCallbackHandler(
  provider: OAuthProviderSpec,
  defaultOptions: AuthHttpOptions = {}
) {
  return function callback(
    ctx: AuthHandlerCtx,
    req: Request,
    options: AuthHttpOptions = defaultOptions
  ): SyncResponse {
    const q = parseQueryString(req.uri);
    const code = q['code'];
    const state = q['state'];
    if (!code || !state) return errorResponse('missing_code_or_state', 400);
    if (
      code.length > MAX_OAUTH_CODE_LENGTH ||
      state.length > MAX_OAUTH_STATE_LENGTH
    ) {
      return errorResponse('invalid_code_or_state', 400);
    }

    let baseUrl: string;
    let clientId: string;
    let clientSecret: string;
    let codeVerifier: string;
    let redirectTo: string;
    try {
      ({ baseUrl, clientId, clientSecret, codeVerifier, redirectTo } =
        ctx.withTx(tx => {
          const cfg = requireConfig(tx);
          const row = tx.db.authOauthState.state.find(state);
          if (!row) throw new BadStateError();
          if (row.provider !== provider.id) throw new BadStateError();
          if (
            row.expiresAt.microsSinceUnixEpoch <
            ctx.timestamp.microsSinceUnixEpoch
          ) {
            tx.db.authOauthState.delete(row);
            throw new BadStateError();
          }
          const out = {
            baseUrl: cfg.baseUrl,
            clientId: provider.getClientId(cfg),
            clientSecret: provider.getClientSecret(cfg),
            codeVerifier: row.codeVerifier,
            redirectTo: row.redirectTo,
          };
          tx.db.authOauthState.delete(row);
          return out;
        }));
    } catch (e) {
      if (e instanceof ConfigMissingError)
        return errorResponse('config_missing', 500);
      if (e instanceof BadStateError) return errorResponse('bad_state', 400);
      throw e;
    }

    const formParams: Record<string, string> = {
      grant_type: 'authorization_code',
      code,
      client_id: clientId,
      client_secret: clientSecret,
      redirect_uri: `${baseUrl}/auth/${provider.id}/callback`,
    };
    if (codeVerifier) formParams['code_verifier'] = codeVerifier;
    const tokenForm = Object.entries(formParams)
      .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
      .join('&');

    const tokRes = ctx.http.fetch(provider.tokenUrl, {
      method: 'POST',
      headers: {
        'content-type': 'application/x-www-form-urlencoded',
        accept: 'application/json',
      },
      body: tokenForm,
    });
    if (!tokRes.ok) return errorResponse('token_exchange_failed', 502);
    const tokens = tokRes.json() as unknown;
    const tokenRecord =
      typeof tokens === 'object' && tokens !== null
        ? (tokens as Record<string, unknown>)
        : {};
    const accessToken =
      typeof tokenRecord.access_token === 'string'
        ? tokenRecord.access_token
        : undefined;
    const refreshToken =
      typeof tokenRecord.refresh_token === 'string'
        ? tokenRecord.refresh_token
        : undefined;
    const idToken =
      typeof tokenRecord.id_token === 'string'
        ? tokenRecord.id_token
        : undefined;
    const expiresIn =
      typeof tokenRecord.expires_in === 'number' &&
      Number.isSafeInteger(tokenRecord.expires_in) &&
      tokenRecord.expires_in > 0
        ? tokenRecord.expires_in
        : undefined;
    if (!accessToken && !idToken)
      return errorResponse('no_token_in_response', 502);

    let profile: OAuthProfile;
    if (provider.resolveProfile && accessToken) {
      const resolved = provider.resolveProfile(ctx, accessToken);
      if (isProfileError(resolved)) return errorResponse(resolved.error, 502);
      profile = resolved;
    } else if (provider.userInfoUrl && accessToken) {
      const uRes = ctx.http.fetch(provider.userInfoUrl, {
        method: 'GET',
        headers: {
          authorization: `Bearer ${accessToken}`,
          accept: 'application/json',
          'user-agent': 'spacetimedb-auth-submodule',
          ...(provider.userInfoHeaders ?? {}),
        },
      });
      if (!uRes.ok) return errorResponse(`userinfo_failed:${uRes.status}`, 502);
      profile = provider.parseProfile(uRes.json());
    } else if (provider.oidc && idToken) {
      return errorResponse('id_token_verification_unsupported', 502);
    } else {
      return errorResponse('cannot_resolve_profile', 502);
    }

    if (
      !profile.email ||
      !profile.sub ||
      profile.email.length > MAX_PROFILE_EMAIL_LENGTH ||
      profile.sub.length > MAX_PROFILE_SUB_LENGTH ||
      (profile.name?.length ?? 0) > MAX_PROFILE_NAME_LENGTH ||
      (profile.image?.length ?? 0) > MAX_PROFILE_IMAGE_LENGTH
    )
      return errorResponse('incomplete_profile', 502);

    const nowMs = Number(ctx.timestamp.microsSinceUnixEpoch / 1000n);
    const newUserId = uuidV7(ctx.random, nowMs);
    const newAccountId = uuidV7(ctx.random, nowMs);
    const sessionId = uuidV7(ctx.random, nowMs);
    const sessionToken = newSessionToken(ctx.random);

    let authResult: {
      issuerUrl: string;
      cookieName: string;
      sessionTtlSeconds: bigint;
      privateKeyPem: string;
      keyId: string;
      userId: string;
    };
    try {
      authResult = ctx.withTx(tx => {
        const cfg = requireConfig(tx);

        let existing: AuthAccount | undefined;
        for (const a of tx.db.authAccount.providerAccountId.filter(
          profile.sub
        )) {
          if (a.providerId === provider.id) {
            existing = a;
            break;
          }
        }

        let resolvedUserId: string;
        if (existing) {
          resolvedUserId = existing.userId;
          tx.db.authAccount.accountId.update({
            ...existing,
            passwordHash: existing.passwordHash,
            accessToken: accessToken ?? existing.accessToken,
            refreshToken: refreshToken ?? existing.refreshToken,
            accessTokenExpiresAt: expiresIn
              ? new Timestamp(
                  ctx.timestamp.microsSinceUnixEpoch +
                    BigInt(expiresIn) * 1_000_000n
                )
              : existing.accessTokenExpiresAt,
            updatedAt: ctx.timestamp,
          });
        } else {
          const byEmail = tx.db.authUser.email.find(
            profile.email.toLowerCase()
          );
          if (byEmail) throw new AccountLinkRequiredError();
          resolvedUserId = newUserId;
          tx.db.authUser.insert({
            userId: newUserId,
            email: profile.email.toLowerCase(),
            emailVerified: profile.emailVerified ?? false,
            name: profile.name,
            image: profile.image,
            createdAt: ctx.timestamp,
            updatedAt: ctx.timestamp,
          });
          tx.db.authAccount.insert({
            accountId: newAccountId,
            userId: resolvedUserId,
            providerId: provider.id,
            providerAccountId: profile.sub,
            passwordHash: undefined,
            accessToken,
            refreshToken,
            accessTokenExpiresAt: expiresIn
              ? new Timestamp(
                  ctx.timestamp.microsSinceUnixEpoch +
                    BigInt(expiresIn) * 1_000_000n
                )
              : undefined,
            createdAt: ctx.timestamp,
            updatedAt: ctx.timestamp,
          });
        }

        tx.db.authSession.insert({
          sessionId,
          userId: resolvedUserId,
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
          sessionTtlSeconds: cfg.sessionTtlSeconds,
          privateKeyPem: cfg.es256PrivateKeyPem,
          keyId: cfg.keyId,
          userId: resolvedUserId,
        };
      });
    } catch (error) {
      if (error instanceof AccountLinkRequiredError) {
        return errorResponse('account_link_required', 409);
      }
      throw error;
    }

    const {
      issuerUrl,
      cookieName,
      sessionTtlSeconds,
      privateKeyPem,
      keyId,
      userId,
    } = authResult;

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

    return redirectResponse(redirectTo, {
      'set-cookie': makeCookie(cookieName, jwt, {
        maxAgeSeconds: ttlSec,
        secure: shouldUseSecureCookies(options.secureCookies),
      }),
    });
  };
}

class BadStateError extends Error {
  constructor() {
    super('bad_state');
  }
}
class AccountLinkRequiredError extends Error {
  constructor() {
    super('account_link_required');
  }
}
class ProviderNotConfiguredError extends Error {
  constructor(public provider: string) {
    super(`provider_not_configured:${provider}`);
  }
}
