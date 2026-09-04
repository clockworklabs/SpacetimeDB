import {
  makeOAuthCallbackHandler,
  makeOAuthStartHandler,
  type OAuthProfile,
  type OAuthProviderSpec,
} from './oauth.ts';
import type { AuthHandlerCtx } from './http.ts';

const githubHeaders = {
  accept: 'application/vnd.github+json',
  'x-github-api-version': '2022-11-28',
  'user-agent': 'spacetimedb-auth-submodule',
};

function record(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null
    ? (value as Record<string, unknown>)
    : {};
}

function pickGithubEmail(rows: unknown): string {
  if (!Array.isArray(rows)) return '';
  const primaryVerified = rows.find(value => {
    const row = record(value);
    return (
      row.primary === true &&
      row.verified === true &&
      typeof row.email === 'string'
    );
  });
  if (primaryVerified) return String(record(primaryVerified).email);
  const verified = rows.find(value => {
    const row = record(value);
    return row.verified === true && typeof row.email === 'string';
  });
  return verified ? String(record(verified).email) : '';
}

function resolveGithubProfile(
  ctx: AuthHandlerCtx,
  accessToken: string
): OAuthProfile | { error: string } {
  const headers = {
    ...githubHeaders,
    authorization: `Bearer ${accessToken}`,
  };

  const userRes = ctx.http.fetch('https://api.github.com/user', {
    method: 'GET',
    headers,
  });
  if (!userRes.ok) return { error: `userinfo_failed:${userRes.status}` };

  const user = record(userRes.json());
  const emailRes = ctx.http.fetch('https://api.github.com/user/emails', {
    method: 'GET',
    headers,
  });
  if (!emailRes.ok) return { error: `github_email_failed:${emailRes.status}` };
  const email = pickGithubEmail(emailRes.json());

  return {
    sub: String(user.id ?? ''),
    email,
    emailVerified: email.length > 0,
    name: typeof user.name === 'string' ? user.name : String(user.login ?? ''),
    image: typeof user.avatar_url === 'string' ? user.avatar_url : undefined,
  };
}

const github: OAuthProviderSpec = {
  id: 'github',
  authorizeUrl: 'https://github.com/login/oauth/authorize',
  tokenUrl: 'https://github.com/login/oauth/access_token',
  scope: 'read:user user:email',
  oidc: false,
  userInfoUrl: 'https://api.github.com/user',
  userInfoHeaders: githubHeaders,
  getClientId: cfg => cfg.githubClientId ?? '',
  getClientSecret: cfg => cfg.githubClientSecret ?? '',
  resolveProfile: resolveGithubProfile,
  parseProfile: data => {
    const user = record(data);
    return {
      sub: String(user.id ?? ''),
      email: String(user.email ?? ''),
      emailVerified: true,
      name:
        typeof user.name === 'string' ? user.name : String(user.login ?? ''),
      image: typeof user.avatar_url === 'string' ? user.avatar_url : undefined,
    };
  },
  usePkce: false,
};

export const githubStartHandler = makeOAuthStartHandler(github);
export const githubCallbackHandler = makeOAuthCallbackHandler(github);
