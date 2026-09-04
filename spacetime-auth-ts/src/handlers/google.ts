import {
  makeOAuthCallbackHandler,
  makeOAuthStartHandler,
  type OAuthProviderSpec,
} from './oauth.ts';

function record(value: unknown): Record<string, unknown> {
  return typeof value === 'object' && value !== null
    ? (value as Record<string, unknown>)
    : {};
}

const google: OAuthProviderSpec = {
  id: 'google',
  authorizeUrl: 'https://accounts.google.com/o/oauth2/v2/auth',
  tokenUrl: 'https://oauth2.googleapis.com/token',
  scope: 'openid email profile',
  oidc: false,
  userInfoUrl: 'https://openidconnect.googleapis.com/v1/userinfo',
  getClientId: cfg => cfg.googleClientId ?? '',
  getClientSecret: cfg => cfg.googleClientSecret ?? '',
  parseProfile: data => {
    const claims = record(data);
    return {
      sub: String(claims.sub ?? ''),
      email: String(claims.email ?? ''),
      emailVerified: claims.email_verified === true,
      name: typeof claims.name === 'string' ? claims.name : undefined,
      image: typeof claims.picture === 'string' ? claims.picture : undefined,
    };
  },
  authorizeExtras: { access_type: 'offline', prompt: 'consent' },
};

export const googleStartHandler = makeOAuthStartHandler(google);
export const googleCallbackHandler = makeOAuthCallbackHandler(google);
