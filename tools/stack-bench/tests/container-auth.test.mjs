import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { containerAuthCommand, resolveContainerAuth,
  SUBSCRIPTION_TOKEN_TARGET } from '../container/container-auth.mjs';

test('container auth keeps direct secret values out of Docker command arguments', () => {
  const secret = 'subscription-secret-value';
  const auth = resolveContainerAuth({ env: { CLAUDE_CODE_OAUTH_TOKEN: secret },
    credentialsPath: '/unused/credentials' });
  assert.equal(auth.mode, 'subscription-token');
  assert.equal(auth.environment.name, 'CLAUDE_CODE_OAUTH_TOKEN');
  assert.equal(auth.environment.value, secret);
  const command = containerAuthCommand(auth, ['--print', 'hello']);
  assert.deepEqual(command, ['claude', '--print', 'hello']);
  assert.doesNotMatch(JSON.stringify(command), /subscription-secret-value/);
});

test('container auth mounts a selected subscription token read-only and loads it at exec', () => {
  const tokenPath = resolve('/private/token');
  const auth = resolveContainerAuth({ env: { CLAUDE_CODE_OAUTH_TOKEN_FILE: tokenPath },
    credentialsPath: '/unused/credentials', exists: path => path === tokenPath,
    read: () => 'present\n' });
  assert.equal(auth.mode, 'subscription-token');
  assert.deepEqual(auth.mount, { kind: 'bind', source: tokenPath,
    target: SUBSCRIPTION_TOKEN_TARGET, readOnly: true });
  const command = containerAuthCommand(auth, ['--print']);
  assert.equal(command[0], 'sh');
  assert.match(command[2], /CLAUDE_CODE_OAUTH_TOKEN/);
  assert.match(command[2], /exec claude/);
  assert.doesNotMatch(JSON.stringify(command), /present/);
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

test('container auth retains the explicit rotating-credential fallback', () => {
  const auth = resolveContainerAuth({ env: {}, credentialsPath: '/home/.claude/.credentials.json',
    exists: path => path === '/home/.claude/.credentials.json' });
  assert.equal(auth.mode, 'credentials');
  assert.equal(auth.mount.target, '/root/.claude/.credentials.json');
  assert.equal(auth.mount.readOnly, false);
});
