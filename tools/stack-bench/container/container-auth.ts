import { readPinnedExecutionCredential } from '../src/agents/credential-profiles.js';
import { existsSync, readFileSync } from 'node:fs';
import type { PathLike } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';

export const SUBSCRIPTION_TOKEN_ENVIRONMENT = 'CLAUDE_CODE_OAUTH_TOKEN';
export const LEGACY_SUBSCRIPTION_TOKEN_TARGET = '/run/secrets/claude-code-oauth-token';

export type ContainerAuth = {
  provider?: 'anthropic' | 'openai' | 'openrouter';
  accountId?: string;
  mode: 'api-key' | 'subscription-token';
  credential: string;
};

type ReadTextFile = (path: PathLike | number, encoding: BufferEncoding) => string;

export interface ResolveContainerAuthOptions {
  provider?: 'anthropic' | 'openai' | 'openrouter';
  apiKey?: string;
  env?: NodeJS.ProcessEnv;
  credentialsPath?: string;
  exists?: (path: PathLike) => boolean;
  read?: ReadTextFile;
}

export function resolveContainerAuth({ provider = 'anthropic', apiKey = '', env = process.env, credentialsPath,
  exists = existsSync, read = readFileSync as ReadTextFile }: ResolveContainerAuthOptions = {}): ContainerAuth {
  const pinned = readPinnedExecutionCredential(env);
  if (pinned) {
    if (pinned.assignment.provider !== provider) throw new Error('Pinned credential provider does not match invocation');
    apiKey = pinned.assignment.mode === 'api-key' ? pinned.secret : '';
    // Resolve the broker credential from the same bytes that passed its pin check.
    // Never reopen a file that an operator can replace between validation and use.
    read = path => {
      if (String(path) !== pinned.secretFile) throw new Error('Pinned credential file does not match invocation');
      return pinned.secret;
    };
  }
  if (provider === 'openrouter') {
    if (!apiKey) throw new Error('OpenRouter requires an API key');
    return { provider, mode: 'api-key', credential: apiKey };
  }
  if (provider === 'openai') {
    const authFile = env.CODEX_AUTH_FILE?.trim();
    if (apiKey && authFile) throw new Error('use only one of OpenAI API-key and account authentication');
    if (apiKey) return { provider, mode: 'api-key', credential: apiKey };
    if (!authFile) throw new Error('OpenAI requires an API key or an explicit CODEX_AUTH_FILE');
    if (!isAbsolute(authFile)) throw new Error('CODEX_AUTH_FILE must be an absolute path');
    if (!exists(authFile)) throw new Error('CODEX_AUTH_FILE does not exist');
    let auth: { auth_mode?: string; OPENAI_API_KEY?: unknown;
      tokens?: { access_token?: unknown; account_id?: unknown } };
    try { auth = JSON.parse(read(authFile, 'utf8')); }
    catch { throw new Error('CODEX_AUTH_FILE must contain valid Codex login JSON'); }
    if (!auth || auth.OPENAI_API_KEY || auth.auth_mode !== 'chatgpt'
      || typeof auth.tokens?.access_token !== 'string' || !auth.tokens.access_token.trim()
      || typeof auth.tokens.account_id !== 'string' || !auth.tokens.account_id.trim()) {
      throw new Error('CODEX_AUTH_FILE must contain ChatGPT account login tokens, not an API key');
    }
    let expiry: unknown;
    try { expiry = JSON.parse(Buffer.from(auth.tokens.access_token.split('.')[1]!, 'base64url').toString()).exp; }
    catch { throw new Error('Codex account access token has no valid expiry; log in again'); }
    if (typeof expiry !== 'number' || !Number.isFinite(expiry) || expiry * 1000 <= Date.now()) {
      throw new Error('Codex account access token is expired; log in again and replace CODEX_AUTH_FILE');
    }
    // Each broker uses an access-token snapshot. It never rotates shared refresh tokens.
    return { provider, mode: 'subscription-token', credential: auth.tokens.access_token,
      accountId: auth.tokens.account_id };
  }
  const token = String(env[SUBSCRIPTION_TOKEN_ENVIRONMENT] ?? '').trim();
  const tokenFileValue = String(env[`${SUBSCRIPTION_TOKEN_ENVIRONMENT}_FILE`] ?? '').trim();
  if (token && tokenFileValue) {
    throw new Error(`use only one of ${SUBSCRIPTION_TOKEN_ENVIRONMENT} and `
      + `${SUBSCRIPTION_TOKEN_ENVIRONMENT}_FILE`);
  }
  if (apiKey && (token || tokenFileValue)) {
    throw new Error('use only one of API-key and subscription-token authentication');
  }
  if (apiKey) return { mode: 'api-key', credential: apiKey };
  if (token) return { mode: 'subscription-token', credential: token };
  if (tokenFileValue) {
    if (!isAbsolute(tokenFileValue)) {
      throw new Error(`${SUBSCRIPTION_TOKEN_ENVIRONMENT}_FILE must be an absolute path`);
    }
    const source = resolve(tokenFileValue);
    if (!exists(source)) throw new Error(`subscription token file does not exist: ${source}`);
    const credential = String(read(source, 'utf8')).trim();
    if (!credential) {
      throw new Error(`subscription token file is empty: ${source}`);
    }
    return { mode: 'subscription-token', credential };
  }
  if (credentialsPath && exists(credentialsPath)) {
    throw new Error('rotating Claude credential files cannot be isolated from generated shell commands; '
      + 'select an API key or CLAUDE_CODE_OAUTH_TOKEN_FILE');
  }
  throw new Error(`no API key, ${SUBSCRIPTION_TOKEN_ENVIRONMENT}, `
    + `${SUBSCRIPTION_TOKEN_ENVIRONMENT}_FILE, or credentials file is available`);
}
