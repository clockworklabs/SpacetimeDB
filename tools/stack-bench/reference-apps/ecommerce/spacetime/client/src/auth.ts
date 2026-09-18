import { UserManager, WebStorageStateStore } from 'oidc-client-ts';
import { OIDC_ISSUER, OIDC_CLIENT_ID, OIDC_REDIRECT_URI } from './config';

export const auth = new UserManager({
  authority: OIDC_ISSUER,
  client_id: OIDC_CLIENT_ID,
  redirect_uri: OIDC_REDIRECT_URI,
  post_logout_redirect_uri: OIDC_REDIRECT_URI,
  response_type: 'code',
  scope: 'openid profile',
  automaticSilentRenew: true,
  userStore: new WebStorageStateStore({ store: window.sessionStorage }),
});

let token: string | undefined;
(window as Window & { getSessionToken?: () => string | null }).getSessionToken = () => token ?? null;
auth.events.addUserLoaded(user => { token = user.expired ? undefined : user.id_token; });
auth.events.addUserUnloaded(() => { token = undefined; });
auth.events.addAccessTokenExpired(() => { void auth.removeUser(); });

export async function initializeAuth(): Promise<string | undefined> {
  const query = new URLSearchParams(location.search);
  if (query.has('state') && (query.has('code') || query.has('error'))) {
    try {
      await auth.signinRedirectCallback();
    } finally {
      history.replaceState(null, '', new URL(OIDC_REDIRECT_URI).pathname);
    }
  }
  const user = await auth.getUser();
  token = user && !user.expired ? user.id_token : undefined;
  return token;
}

export async function logout(): Promise<void> {
  const user = await auth.getUser();
  await auth.removeUser();
  // Logout removes this browser's credential and ends its provider session.
  // It does not promise immediate revocation of previously issued bearer tokens.
  await auth.signoutRedirect({ id_token_hint: user?.id_token });
}
