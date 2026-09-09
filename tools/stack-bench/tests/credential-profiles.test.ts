import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { assertExecutionCredentialUnchanged, resolveExecutionCredentials, executionCredentialsSchema }
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
      STACK_BENCH_API_KEY_FILE: '/generic', STACK_BENCH_AGENT_API_KEY: 'generic', PATH: '/bin' };
    assert.deepEqual(resolveExecutionCredentials('claude-code', 'a', undefined, source), { env: source, assignment: null });
    const selected = resolveExecutionCredentials('claude-code', 'a', { default: 'first',
      adapters: { 'claude-code': 'second' }, attempts: { a: 'first' } }, source);
    assert.deepEqual(selected.assignment, { id: 'first', version: 'v1', provider: 'anthropic', mode: 'subscription-token' });
    assert.equal(selected.env.CLAUDE_CODE_OAUTH_TOKEN_FILE, secretFile);
    assert.equal(selected.env.ANTHROPIC_API_KEY, undefined);
    assert.equal(selected.env.STACK_BENCH_API_KEY_FILE, undefined);
    assert.equal(selected.env.STACK_BENCH_AGENT_API_KEY, undefined);
    assert.equal(selected.env.OPENAI_API_KEY_FILE, '/other-provider');
    assert.equal(source.ANTHROPIC_API_KEY, 'ambient');
    assert.doesNotMatch(JSON.stringify(selected.assignment), /SYNTHETIC|secretFile/);
    assert.doesNotThrow(() => assertExecutionCredentialUnchanged(selected.env));
    assert.equal(resolveExecutionCredentials('claude-code', 'b', { default: 'first',
      adapters: { 'claude-code': 'second' } }, source).assignment?.id, 'second');
    assert.equal(resolveExecutionCredentials('codex', 'a', { default: 'openai' }, source).env.CODEX_AUTH_FILE, secretFile);
    assert.equal(resolveExecutionCredentials('openrouter', 'a', { default: 'router' }, source).env.OPENROUTER_API_KEY_FILE, secretFile);
    assert.throws(() => resolveExecutionCredentials('codex', 'a', { default: 'first' }, source), /does not match/);
    assert.throws(() => resolveExecutionCredentials('claude-code', 'a', { default: 'absent' }, source), /not registered/);
    assert.throws(() => executionCredentialsSchema.parse({ default: 'first', secret: 'unwanted' }));
    writeFileSync(secretFile, 'SYNTHETIC_SECRET_TWO');
    assert.throws(() => assertExecutionCredentialUnchanged(selected.env), /changed after admission/);
    writeFileSync(secretFile, 'SYNTHETIC_SECRET_ONE');
    profiles.first.version = 'v3';
    writeFileSync(registry, JSON.stringify(profiles));
    assert.throws(() => assertExecutionCredentialUnchanged(selected.env), /changed after admission/);
    writeFileSync(registry, '{not-json SYNTHETIC_SECRET_TWO');
    assert.throws(() => resolveExecutionCredentials('claude-code', 'a', { default: 'first' }, source),
      error => error instanceof Error && !error.message.includes('SYNTHETIC'));
  } finally { rmSync(root, { recursive: true, force: true }); }
});
