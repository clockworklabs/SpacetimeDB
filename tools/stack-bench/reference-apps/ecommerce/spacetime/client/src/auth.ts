import { MODULE_NAME, SPACETIMEDB_URI } from './config';

const TOKEN_KEY = `${MODULE_NAME}-identity`;
const ERROR_KEY = `${MODULE_NAME}-auth-error`;
const base = SPACETIMEDB_URI.replace(/^ws/, 'http');
export const savedToken = () => sessionStorage.getItem(TOKEN_KEY) ?? undefined;
export const saveToken = (token: string) => sessionStorage.setItem(TOKEN_KEY, token);
(window as Window & { getSessionToken?: () => string | null }).getSessionToken = () => savedToken() ?? null;

export const readAuthError = () => sessionStorage.getItem(ERROR_KEY);

export async function authenticate(mode: 'signup' | 'signin', name: string, password: string): Promise<void> {
  sessionStorage.removeItem(ERROR_KEY);
  // Rotate the native credential before authentication: never upgrade a guest token.
  const identityResponse = await fetch(`${base}/v1/identity`, { method: 'POST' });
  if (!identityResponse.ok) throw new Error('Could not start login.');
  const identity = await identityResponse.json();
  if (typeof identity.token !== 'string' || !identity.token) throw new Error('Invalid identity response.');
  saveToken(identity.token);
  try {
    const salt = Array.from(crypto.getRandomValues(new Uint8Array(32)), byte => byte.toString(16).padStart(2, '0')).join('');
    const response = await fetch(`${base}/v1/database/${encodeURIComponent(MODULE_NAME)}/call/${mode === 'signup' ? 'sign_up' : 'sign_in'}`, {
      method: 'POST', headers: { 'content-type': 'application/json', Authorization: `Bearer ${identity.token}` },
      body: JSON.stringify(mode === 'signup' ? [name, password, salt] : [name, password]),
    });
    if (!response.ok || await response.json() !== true) {
      throw new Error(mode === 'signup' ? 'Could not create this account.' : 'Invalid username or password.');
    }
  } catch (error) {
    sessionStorage.setItem(ERROR_KEY, error instanceof Error ? error.message : 'Login failed.');
  }
  // Reconnect with the submitted credential even after failure. The server account
  // view and protected writes decide whether it has any application permissions.
  location.reload();
}

export function clearToken(): void { sessionStorage.removeItem(TOKEN_KEY); }
