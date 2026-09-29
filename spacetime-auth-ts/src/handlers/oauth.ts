import type { SyncResponse, Request } from 'spacetimedb/server';
import { Timestamp } from 'spacetimedb';
import { pkceChallenge } from '../crypto';
import type { AuthHandlerCtx } from '../context';
import type { AuthHttpOptions } from '../client';
import {
  clearCookie,
  HttpError,
  makeCookie,
  parseCookies,
  parseQueryString,
  redirectResponse,
  requireConfig,
} from './http';
import { AUTH_RATE_LIMITS, enforceRateLimits } from '../rate_limit';
import {
  clientKey,
  safeRedirectPath,
  shouldUseSecureCookies,
} from '../request-trust';
import { issueSession } from '../sessions';
import { newId, newToken } from '../tokens';
import type { AuthAccount, AuthConfig } from '../types';

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

type Handler = (
  ctx: AuthHandlerCtx,
  req: Request,
  options: AuthHttpOptions
) => SyncResponse;

// Binds the OAuth state to the browser that started the flow, so a callback
// URL carrying someone else's code cannot sign this browser in.
function stateCookieName(cfg: AuthConfig): string {
  return `${cfg.cookieName}_oauth_state`;
}

function callbackUrl(cfg: AuthConfig, provider: OAuthProviderSpec): string {
  return `${cfg.baseUrl}/auth/${provider.id}/callback`;
}

function formEncode(params: Record<string, string>): string {
  return Object.entries(params)
    .map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`)
    .join('&');
}

export function oauthStart(provider: OAuthProviderSpec): Handler {
  return (ctx, req, options) => {
    const requestedRedirect = parseQueryString(req.uri)['redirectTo'];
    const redirectTo =
      requestedRedirect === undefined
        ? '/'
        : safeRedirectPath(requestedRedirect);
    if (redirectTo === undefined) throw new HttpError('invalid_redirect', 400);
    const ipKey = clientKey(req, options.trustedProxyHeader);
    const limited = ipKey
      ? enforceRateLimits(ctx, [
          {
            policy: AUTH_RATE_LIMITS.oauthStart,
            actor: `ip:${ipKey}:${provider.id}`,
          },
        ])
      : null;
    if (limited) return limited;

    const usePkce = provider.usePkce ?? true;
    const { cfg, clientId, state, verifier } = ctx.withTx(tx => {
      const cfg = requireConfig(tx);
      const clientId = provider.getClientId(cfg);
      if (!clientId)
        throw new HttpError(`provider_not_configured:${provider.id}`, 500);
      const state = newToken(tx);
      const verifier = usePkce ? newToken(tx) : '';
      tx.db.authOauthState.insert({
        state,
        provider: provider.id,
        codeVerifier: verifier,
        redirectTo,
        expiresAt: new Timestamp(
          tx.timestamp.microsSinceUnixEpoch +
            OAUTH_STATE_TTL_SECONDS * 1_000_000n
        ),
        createdAt: tx.timestamp,
      });
      return { cfg, clientId, state, verifier };
    });

    const params: Record<string, string> = {
      client_id: clientId,
      redirect_uri: callbackUrl(cfg, provider),
      response_type: 'code',
      scope: provider.scope,
      state,
    };
    if (usePkce) {
      params['code_challenge'] = pkceChallenge(verifier);
      params['code_challenge_method'] = 'S256';
    }
    Object.assign(params, provider.authorizeExtras);
    const sep = provider.authorizeUrl.includes('?') ? '&' : '?';
    return redirectResponse(
      `${provider.authorizeUrl}${sep}${formEncode(params)}`,
      [
        makeCookie(stateCookieName(cfg), state, {
          maxAgeSeconds: Number(OAUTH_STATE_TTL_SECONDS),
          secure: shouldUseSecureCookies(options.secureCookies),
        }),
      ]
    );
  };
}

export function oauthCallback(provider: OAuthProviderSpec): Handler {
  return (ctx, req, options) => {
    const q = parseQueryString(req.uri);
    const code = q['code'];
    const state = q['state'];
    if (!code || !state) throw new HttpError('missing_code_or_state', 400);
    if (
      code.length > MAX_OAUTH_CODE_LENGTH ||
      state.length > MAX_OAUTH_STATE_LENGTH
    ) {
      throw new HttpError('invalid_code_or_state', 400);
    }

    const { cfg, codeVerifier, redirectTo } = ctx.withTx(tx => {
      const cfg = requireConfig(tx);
      const row = tx.db.authOauthState.state.find(state);
      const cookieState = parseCookies(req.headers.get('cookie'))[
        stateCookieName(cfg)
      ];
      if (
        !row ||
        row.provider !== provider.id ||
        row.expiresAt.microsSinceUnixEpoch <
          tx.timestamp.microsSinceUnixEpoch ||
        cookieState !== state
      ) {
        throw new HttpError('bad_state', 400);
      }
      tx.db.authOauthState.delete(row);
      return {
        cfg,
        codeVerifier: row.codeVerifier,
        redirectTo: row.redirectTo,
      };
    });

    const formParams: Record<string, string> = {
      grant_type: 'authorization_code',
      code,
      client_id: provider.getClientId(cfg),
      client_secret: provider.getClientSecret(cfg),
      redirect_uri: callbackUrl(cfg, provider),
    };
    if (codeVerifier) formParams['code_verifier'] = codeVerifier;

    const tokRes = ctx.http.fetch(provider.tokenUrl, {
      method: 'POST',
      headers: {
        'content-type': 'application/x-www-form-urlencoded',
        accept: 'application/json',
      },
      body: formEncode(formParams),
    });
    if (!tokRes.ok) throw new HttpError('token_exchange_failed', 502);
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
      throw new HttpError('no_token_in_response', 502);

    let profile: OAuthProfile;
    if (provider.resolveProfile && accessToken) {
      const resolved = provider.resolveProfile(ctx, accessToken);
      if (isProfileError(resolved)) throw new HttpError(resolved.error, 502);
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
      if (!uRes.ok) throw new HttpError(`userinfo_failed:${uRes.status}`, 502);
      profile = provider.parseProfile(uRes.json());
    } else if (provider.oidc && idToken) {
      throw new HttpError('id_token_verification_unsupported', 502);
    } else {
      throw new HttpError('cannot_resolve_profile', 502);
    }

    if (
      !profile.email ||
      !profile.sub ||
      profile.email.length > MAX_PROFILE_EMAIL_LENGTH ||
      profile.sub.length > MAX_PROFILE_SUB_LENGTH ||
      (profile.name?.length ?? 0) > MAX_PROFILE_NAME_LENGTH ||
      (profile.image?.length ?? 0) > MAX_PROFILE_IMAGE_LENGTH
    )
      throw new HttpError('incomplete_profile', 502);
    const email = profile.email.toLowerCase();

    const session = ctx.withTx(tx => {
      const cfg = requireConfig(tx);
      const accessTokenExpiresAt = expiresIn
        ? new Timestamp(
            tx.timestamp.microsSinceUnixEpoch + BigInt(expiresIn) * 1_000_000n
          )
        : undefined;

      let existing: AuthAccount | undefined;
      for (const a of tx.db.authAccount.providerAccountId.filter(profile.sub)) {
        if (a.providerId === provider.id) {
          existing = a;
          break;
        }
      }

      let userId: string;
      if (existing) {
        userId = existing.userId;
        tx.db.authAccount.accountId.update({
          ...existing,
          accessToken: accessToken ?? existing.accessToken,
          refreshToken: refreshToken ?? existing.refreshToken,
          accessTokenExpiresAt:
            accessTokenExpiresAt ?? existing.accessTokenExpiresAt,
          updatedAt: tx.timestamp,
        });
      } else {
        const byEmail = tx.db.authUser.email.find(email);
        if (byEmail) {
          // Link only when both sides proved control of the address. An
          // unverified local account may belong to someone who registered the
          // address without owning it.
          if (!profile.emailVerified || !byEmail.emailVerified)
            throw new HttpError('account_link_required', 409);
          userId = byEmail.userId;
        } else {
          userId = newId(tx);
          tx.db.authUser.insert({
            userId,
            email,
            emailVerified: profile.emailVerified ?? false,
            name: profile.name,
            image: profile.image,
            createdAt: tx.timestamp,
            updatedAt: tx.timestamp,
          });
        }
        tx.db.authAccount.insert({
          accountId: newId(tx),
          userId,
          providerId: provider.id,
          providerAccountId: profile.sub,
          passwordHash: undefined,
          accessToken,
          refreshToken,
          accessTokenExpiresAt,
          createdAt: tx.timestamp,
          updatedAt: tx.timestamp,
        });
      }

      return issueSession(tx, cfg, userId, req, options);
    });

    return redirectResponse(redirectTo, [
      session.cookie,
      clearCookie(
        stateCookieName(cfg),
        shouldUseSecureCookies(options.secureCookies)
      ),
    ]);
  };
}
