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
