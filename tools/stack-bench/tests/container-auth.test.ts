import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { resolveContainerAuth } from '../container/container-auth.js';

test('OpenAI account auth extracts only an unexpired access token and rejects API fallback', () => {
  const token = `header.${Buffer.from(JSON.stringify({ exp: Math.floor(Date.now() / 1000) + 3600 })).toString('base64url')}.signature`;
  const options = { provider: 'openai' as const, env: { CODEX_AUTH_FILE: resolve('/private/auth.json') },
    exists: () => true, read: () => JSON.stringify({ auth_mode: 'chatgpt',
      tokens: { access_token: token, refresh_token: 'never-forward', account_id: 'account' } }) };
  assert.deepEqual(resolveContainerAuth(options), { provider: 'openai', mode: 'subscription-token',
    credential: token, accountId: 'account' });
  assert.throws(() => resolveContainerAuth({ ...options, apiKey: 'key' }), /only one/);
  assert.throws(() => resolveContainerAuth({ ...options, read: () => '{broken' }), /valid Codex/);
  assert.throws(() => resolveContainerAuth({ ...options, read: () => JSON.stringify({
    OPENAI_API_KEY: 'key' }) }), /not an API key/);
  assert.throws(() => resolveContainerAuth({ ...options, read: () => JSON.stringify({ auth_mode: 'chatgpt',
    tokens: { access_token: 'header.eyJleHAiOjF9.signature', account_id: 'account' } }) }), /expired/);
  assert.deepEqual(resolveContainerAuth({ provider: 'openai', env: {}, apiKey: 'key' }),
    { provider: 'openai', mode: 'api-key', credential: 'key' });
});

test('container auth resolves a direct subscription token in controller memory', () => {
  const secret = 'subscription-secret-value';
  const auth = resolveContainerAuth({ env: { CLAUDE_CODE_OAUTH_TOKEN: secret },
    credentialsPath: '/unused/credentials' });
  assert.deepEqual(auth, { mode: 'subscription-token', credential: secret });
});

test('container auth resolves a selected subscription token only in the controller', () => {
  const tokenPath = resolve('/private/token');
  const auth = resolveContainerAuth({ env: { CLAUDE_CODE_OAUTH_TOKEN_FILE: tokenPath },
    credentialsPath: '/unused/credentials', exists: path => path === tokenPath,
    read: () => 'present\n' });
  assert.deepEqual(auth, { mode: 'subscription-token', credential: 'present' });
});

test('container auth rejects ambiguous or unusable selected credentials', () => {
  assert.throws(() => resolveContainerAuth({ apiKey: 'key',
    env: { CLAUDE_CODE_OAUTH_TOKEN: 'token' } }), /only one/);
  assert.throws(() => resolveContainerAuth({ env: {
    CLAUDE_CODE_OAUTH_TOKEN: 'token', CLAUDE_CODE_OAUTH_TOKEN_FILE: '/private/token',
  } }), /only one/);
  assert.throws(() => resolveContainerAuth({ env: {
    CLAUDE_CODE_OAUTH_TOKEN_FILE: 'relative-token',
  } }), /absolute path/);
  assert.throws(() => resolveContainerAuth({ env: {
    CLAUDE_CODE_OAUTH_TOKEN_FILE: '/private/token',
  }, exists: () => true, read: () => '\n' }), /is empty/);
});

test('container auth rejects rotating credentials that generated commands could read', () => {
  assert.throws(() => resolveContainerAuth({ env: {},
    credentialsPath: '/home/.claude/.credentials.json',
    exists: path => path === '/home/.claude/.credentials.json' }),
  /cannot be isolated/);
});


test('OpenRouter accepts only its explicit API key and never falls back to account credentials', () => {
  assert.throws(() => resolveContainerAuth({ provider: 'openrouter',
    env: { CLAUDE_CODE_OAUTH_TOKEN: 'other-provider-token', CODEX_AUTH_FILE: '/other-provider-login' } }),
  /OpenRouter requires an API key/);
  assert.deepEqual(resolveContainerAuth({ provider: 'openrouter', apiKey: 'router-key', env: {} }),
    { provider: 'openrouter', mode: 'api-key', credential: 'router-key' });
});
