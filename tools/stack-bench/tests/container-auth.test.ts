import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { resolveContainerAuth } from '../container/container-auth.js';

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
