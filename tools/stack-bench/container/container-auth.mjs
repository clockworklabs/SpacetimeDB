import { existsSync, readFileSync } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';

export const SUBSCRIPTION_TOKEN_ENVIRONMENT = 'CLAUDE_CODE_OAUTH_TOKEN';
export const SUBSCRIPTION_TOKEN_TARGET = '/run/secrets/claude-code-oauth-token';

export function resolveContainerAuth({ apiKey = '', env = process.env, credentialsPath,
  exists = existsSync, read = readFileSync } = {}) {
  const token = String(env[SUBSCRIPTION_TOKEN_ENVIRONMENT] ?? '').trim();
  const tokenFileValue = String(env[`${SUBSCRIPTION_TOKEN_ENVIRONMENT}_FILE`] ?? '').trim();
  if (token && tokenFileValue) {
    throw new Error(`use only one of ${SUBSCRIPTION_TOKEN_ENVIRONMENT} and `
      + `${SUBSCRIPTION_TOKEN_ENVIRONMENT}_FILE`);
  }
  if (apiKey && (token || tokenFileValue)) {
    throw new Error('use only one of API-key and subscription-token authentication');
  }
  if (apiKey) return { mode: 'api-key',
    environment: { name: 'ANTHROPIC_API_KEY', value: apiKey }, mount: null };
  if (token) return { mode: 'subscription-token',
    environment: { name: SUBSCRIPTION_TOKEN_ENVIRONMENT, value: token }, mount: null };
  if (tokenFileValue) {
    if (!isAbsolute(tokenFileValue)) {
      throw new Error(`${SUBSCRIPTION_TOKEN_ENVIRONMENT}_FILE must be an absolute path`);
    }
    const source = resolve(tokenFileValue);
    if (!exists(source)) throw new Error(`subscription token file does not exist: ${source}`);
    if (!String(read(source, 'utf8')).trim()) {
      throw new Error(`subscription token file is empty: ${source}`);
    }
    return { mode: 'subscription-token', environment: null,
      mount: { kind: 'bind', source, target: SUBSCRIPTION_TOKEN_TARGET, readOnly: true } };
  }
  if (credentialsPath && exists(credentialsPath)) {
    return { mode: 'credentials', environment: null,
      mount: { kind: 'bind', source: credentialsPath,
        target: '/root/.claude/.credentials.json', readOnly: false } };
  }
  throw new Error(`no API key, ${SUBSCRIPTION_TOKEN_ENVIRONMENT}, `
    + `${SUBSCRIPTION_TOKEN_ENVIRONMENT}_FILE, or credentials file is available`);
}

export function containerAuthCommand(auth, claudeArgs) {
  if (auth.mode !== 'subscription-token' || !auth.mount) return ['claude', ...claudeArgs];
  return ['sh', '-lc',
    `CLAUDE_CODE_OAUTH_TOKEN="$(cat ${SUBSCRIPTION_TOKEN_TARGET})" exec claude "$@"`,
    'stack-bench-claude', ...claudeArgs];
}
