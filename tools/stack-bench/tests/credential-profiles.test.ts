import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { resolveContainerAuth } from '../container/container-auth.js';
import { readPinnedExecutionCredential, resolveExecutionCredentials, executionCredentialsSchema, validateExecutionCredentialTargets }
  from '../src/agents/credential-profiles.js';

test('named execution credentials select explicitly, preserve other providers, and fail closed on drift', () => {
  const root = mkdtempSync(join(tmpdir(), 'credential-profiles-'));
  try {
    const secretFile = join(root, 'secret');
    const registry = join(root, 'profiles.json');
    writeFileSync(secretFile, 'SYNTHETIC_SECRET_ONE');
    const profiles = {
      first: { provider: 'anthropic', mode: 'subscription-token', secretFile, version: 'v1' },
      second: { provider: 'anthropic', mode: 'api-key', secretFile, version: 'v2' },
      openai: { provider: 'openai', mode: 'subscription-token', secretFile, version: 'v1' },
      router: { provider: 'openrouter', mode: 'api-key', secretFile, version: 'v1' },
    };
    writeFileSync(registry, JSON.stringify(profiles));
    const source = { STACK_BENCH_CREDENTIAL_PROFILES_FILE: registry,
      ANTHROPIC_API_KEY: 'ambient', OPENAI_API_KEY_FILE: '/other-provider',
      STACK_BENCH_API_KEY_FILE: '/generic', STACK_BENCH_AGENT_API_KEY: 'generic', STACK_BENCH_AGENT_API_KEY_FILE: '/stale-agent-key', PATH: '/bin' };
    assert.deepEqual(resolveExecutionCredentials('claude-code', 'a', undefined, source), { env: source, assignment: null });
    const selected = resolveExecutionCredentials('claude-code', 'a', { default: 'first',
      adapters: { 'claude-code': 'second' }, attempts: { a: 'first' } }, source);
    assert.deepEqual(selected.assignment, { id: 'first', version: 'v1', provider: 'anthropic', mode: 'subscription-token' });
    assert.equal(selected.env.CLAUDE_CODE_OAUTH_TOKEN_FILE, secretFile);
    assert.equal(selected.env.ANTHROPIC_API_KEY, undefined);
    assert.equal(selected.env.STACK_BENCH_API_KEY_FILE, undefined);
    assert.equal(selected.env.STACK_BENCH_AGENT_API_KEY, undefined);
    assert.equal(selected.env.STACK_BENCH_AGENT_API_KEY_FILE, undefined);
    assert.equal(selected.env.OPENAI_API_KEY_FILE, '/other-provider');
    assert.equal(source.ANTHROPIC_API_KEY, 'ambient');
    assert.doesNotMatch(JSON.stringify(selected.assignment), /SYNTHETIC|secretFile/);
    assert.doesNotThrow(() => readPinnedExecutionCredential(selected.env));
    const auth = resolveContainerAuth({ provider: 'anthropic', env: selected.env,
      read: () => { throw new Error('must not reopen the secret file after checking its pin'); } });
    assert.equal(auth.credential, 'SYNTHETIC_SECRET_ONE');
    const raced = resolveContainerAuth({ provider: 'anthropic', env: selected.env,
      exists: () => { writeFileSync(secretFile, 'SYNTHETIC_REPLACEMENT'); return true; } });
    assert.equal(raced.credential, 'SYNTHETIC_SECRET_ONE', 'broker uses the verified snapshot, not a replacement file');
    assert.throws(() => resolveContainerAuth({ provider: 'anthropic', env: selected.env }), /changed after admission/);
    writeFileSync(secretFile, 'SYNTHETIC_SECRET_ONE');

    const api = resolveExecutionCredentials('claude-code', 'b', { default: 'second' }, source);
    assert.equal(resolveContainerAuth({ provider: 'anthropic', env: api.env,
      apiKey: 'stale-caller-key' }).credential, 'SYNTHETIC_SECRET_ONE');
    assert.throws(() => resolveContainerAuth({ provider: 'openai', env: api.env }), /provider does not match/);
    assert.doesNotThrow(() => validateExecutionCredentialTargets({ attempts: { a: 'first' } }, ['claude-code'], ['a']));
    assert.throws(() => validateExecutionCredentialTargets({ attempts: { typo: 'first' } }, ['claude-code'], ['a']), /unknown attempt/);
    assert.throws(() => validateExecutionCredentialTargets({ adapters: { typo: 'first' } }, ['claude-code'], ['a']), /unknown adapter/);

    assert.equal(resolveExecutionCredentials('claude-code', 'b', { default: 'first',
      adapters: { 'claude-code': 'second' } }, source).assignment?.id, 'second');
    assert.equal(resolveExecutionCredentials('codex', 'a', { default: 'openai' }, source).env.CODEX_AUTH_FILE, secretFile);
    assert.equal(resolveExecutionCredentials('openrouter', 'a', { default: 'router' }, source).env.OPENROUTER_API_KEY_FILE, secretFile);
    assert.throws(() => resolveExecutionCredentials('codex', 'a', { default: 'first' }, source), /does not match/);
    assert.throws(() => resolveExecutionCredentials('claude-code', 'a', { default: 'absent' }, source), /not registered/);
    assert.throws(() => executionCredentialsSchema.parse({ default: 'first', secret: 'unwanted' }));
    writeFileSync(secretFile, 'SYNTHETIC_SECRET_TWO');
    assert.throws(() => readPinnedExecutionCredential(selected.env), /changed after admission/);
    assert.throws(() => resolveContainerAuth({ provider: 'anthropic', env: selected.env }), /changed after admission/);
    writeFileSync(secretFile, 'SYNTHETIC_SECRET_ONE');
    profiles.first.version = 'v3';
    writeFileSync(registry, JSON.stringify(profiles));
    assert.throws(() => readPinnedExecutionCredential(selected.env), /changed after admission/);
    writeFileSync(registry, '{not-json SYNTHETIC_SECRET_TWO');
    assert.throws(() => resolveExecutionCredentials('claude-code', 'a', { default: 'first' }, source),
      error => error instanceof Error && !error.message.includes('SYNTHETIC'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('simultaneous account assignments keep independent secrets and drift checks', () => {
  const root = mkdtempSync(join(tmpdir(), 'credential-isolation-'));
  try {
    const registry = join(root, 'profiles.json');
    const firstFile = join(root, 'first');
    const secondFile = join(root, 'second');
    writeFileSync(firstFile, 'SYNTHETIC_FIRST_ACCOUNT');
    writeFileSync(secondFile, 'SYNTHETIC_SECOND_ACCOUNT');
    writeFileSync(registry, JSON.stringify({
      first: { provider: 'anthropic', mode: 'api-key', secretFile: firstFile, version: 'v1' },
      second: { provider: 'anthropic', mode: 'api-key', secretFile: secondFile, version: 'v1' },
    }));
    const source = { STACK_BENCH_CREDENTIAL_PROFILES_FILE: registry, ANTHROPIC_API_KEY: 'ambient' };
    const first = resolveExecutionCredentials('claude-code', 'a', { default: 'first' }, source);
    const second = resolveExecutionCredentials('claude-code', 'a', { default: 'second' }, source);
    assert.equal(readPinnedExecutionCredential(first.env)?.secret, 'SYNTHETIC_FIRST_ACCOUNT');
    assert.equal(readPinnedExecutionCredential(second.env)?.secret, 'SYNTHETIC_SECOND_ACCOUNT');
    writeFileSync(firstFile, 'SYNTHETIC_CHANGED_ACCOUNT');
    assert.throws(() => readPinnedExecutionCredential(first.env), /changed after admission/);
    assert.equal(readPinnedExecutionCredential(second.env)?.secret, 'SYNTHETIC_SECOND_ACCOUNT');
    assert.deepEqual(source, { STACK_BENCH_CREDENTIAL_PROFILES_FILE: registry, ANTHROPIC_API_KEY: 'ambient' });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a Grok sign-in profile stays pinned to its account while its tokens rotate', () => {
  const root = mkdtempSync(join(tmpdir(), 'credential-profiles-grok-'));
  try {
    const secretFile = join(root, 'grok_auth');
    const registry = join(root, 'profiles.json');
    const login = (key: string, principal = 'user-1') => JSON.stringify({ 'https://auth.x.ai::client': { key,
      refresh_token: `refresh-${key}`, expires_at: '2026-10-06T23:00:00Z', principal_id: principal,
      oidc_issuer: 'https://auth.x.ai', oidc_client_id: 'client' } });
    writeFileSync(secretFile, login('first'));
    writeFileSync(registry, JSON.stringify({ grok: { provider: 'xai', mode: 'subscription-token', secretFile, version: 'v1' } }));
    const selected = resolveExecutionCredentials('grok-build', 'a', { default: 'grok' },
      { STACK_BENCH_CREDENTIAL_PROFILES_FILE: registry, XAI_API_KEY: 'ambient' });
    assert.equal(selected.env.GROK_AUTH_FILE, secretFile);
    assert.equal(selected.env.XAI_API_KEY, undefined);
    writeFileSync(secretFile, login('refreshed'));
    assert.equal(readPinnedExecutionCredential(selected.env)!.secret, login('refreshed'));
    writeFileSync(secretFile, login('other', 'user-2'));
    assert.throws(() => readPinnedExecutionCredential(selected.env), /changed after admission/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('an upstream profile is an Anthropic API key, pinned with its endpoint', () => {
  const root = mkdtempSync(join(tmpdir(), 'credential-profiles-'));
  try {
    const secretFile = join(root, 'secret');
    const registry = join(root, 'profiles.json');
    writeFileSync(secretFile, 'SYNTHETIC_SECRET_ZAI');
    const profiles: Record<string, unknown> = {
      zai: { provider: 'anthropic', mode: 'api-key', upstream: 'zai', secretFile, version: 'v1' },
      bad: { provider: 'anthropic', mode: 'subscription-token', upstream: 'zai', secretFile, version: 'v1' },
    };
    writeFileSync(registry, JSON.stringify(profiles));
    const source = { STACK_BENCH_CREDENTIAL_PROFILES_FILE: registry };
    const selected = resolveExecutionCredentials('claude-code', 'a', { default: 'zai' }, source);
    assert.deepEqual(selected.assignment, { id: 'zai', version: 'v1', provider: 'anthropic', mode: 'api-key', upstream: 'zai' });
    assert.deepEqual(resolveContainerAuth({ provider: 'anthropic', env: selected.env }),
      { mode: 'api-key', credential: 'SYNTHETIC_SECRET_ZAI', upstream: 'zai' });
    assert.throws(() => resolveExecutionCredentials('claude-code', 'a', { default: 'bad' }, source), /invalid/);
    profiles.zai = { ...profiles.zai as object, upstream: 'deepseek' };
    writeFileSync(registry, JSON.stringify(profiles));
    assert.throws(() => readPinnedExecutionCredential(selected.env), /changed after admission/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a Google profile is a Gemini API key or an agy sign-in pinned by its account', () => {
  const root = mkdtempSync(join(tmpdir(), 'credential-profiles-'));
  try {
    const secretFile = join(root, 'secret');
    const loginFile = join(root, 'agy_auth');
    const registry = join(root, 'profiles.json');
    const claims = Buffer.from(JSON.stringify({ iss: 'https://accounts.google.com', sub: 'user-1' })).toString('base64url');
    const login = (access: string) => JSON.stringify({ token: { access_token: access, refresh_token: '1//r',
      expiry: '2026-10-09T13:00:00Z' }, auth_method: 'consumer', id_token: `h.${claims}.s` });
    writeFileSync(secretFile, 'SYNTHETIC_SECRET_GEMINI');
    writeFileSync(loginFile, login('ya29.one'));
    writeFileSync(registry, JSON.stringify({
      gemini: { provider: 'google', mode: 'api-key', secretFile, version: 'v1' },
      account: { provider: 'google', mode: 'subscription-token', secretFile: loginFile, version: 'v1' } }));
    const source = { STACK_BENCH_CREDENTIAL_PROFILES_FILE: registry };
    const selected = resolveExecutionCredentials('antigravity', 'a', { default: 'gemini' }, source);
    assert.equal(selected.env.GEMINI_API_KEY_FILE, secretFile);
    assert.deepEqual(resolveContainerAuth({ provider: 'google', env: selected.env }),
      { provider: 'google', mode: 'api-key', credential: 'SYNTHETIC_SECRET_GEMINI' });
    const account = resolveExecutionCredentials('antigravity', 'a', { default: 'account' }, source);
    assert.equal(account.env.AGY_AUTH_FILE, loginFile);
    assert.deepEqual(resolveContainerAuth({ provider: 'google', env: account.env }),
      { provider: 'google', mode: 'subscription-token', credential: 'ya29.one' });
    // A renewed token is the same account; the pin holds.
    writeFileSync(loginFile, login('ya29.two'));
    assert.equal(resolveContainerAuth({ provider: 'google', env: account.env }).credential, 'ya29.two');
  } finally { rmSync(root, { recursive: true, force: true }); }
});
