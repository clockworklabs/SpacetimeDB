import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { ensureFreshGrokLogin, readGrokLogin } from '../src/agents/grok-login.js';

const HOUR = 3_600_000;
const NOW = Date.parse('2026-10-06T12:00:00Z');
const SCOPE = 'https://auth.x.ai::client-1';

function login(expiresAt: number, extra: Record<string, unknown> = {}) {
  return { [SCOPE]: { key: 'access-1', auth_mode: 'oidc', refresh_token: 'refresh-1', principal_id: 'user-1',
    expires_at: new Date(expiresAt).toISOString(), oidc_issuer: 'https://auth.x.ai', oidc_client_id: 'client-1',
    email: 'kept@example.com', ...extra } };
}

function fixture(content: unknown) {
  const root = mkdtempSync(join(tmpdir(), 'grok-login-'));
  const path = join(root, 'auth.json');
  writeFileSync(path, JSON.stringify(content), { mode: 0o600 });
  return { root, path };
}

test('a fresh Grok login is used as it is', async () => {
  const { root, path } = fixture(login(NOW + 6 * HOUR));
  try {
    const result = await ensureFreshGrokLogin(path, 4.5 * HOUR,
      { now: () => NOW, fetch: () => assert.fail('no refresh while enough time remains') });
    assert.equal(result.token, 'access-1');
    assert.equal(result.expiresAtMs, NOW + 6 * HOUR);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('an expiring Grok login is refreshed once, rotated in place, and keeps its account identity', async () => {
  const { root, path } = fixture(login(NOW + HOUR));
  try {
    const before = readGrokLogin(readFileSync(path, 'utf8'));
    const requests: { url: string; body: string }[] = [];
    const exchange = async (url: string, init: { body: string }) => {
      requests.push({ url, body: init.body });
      await new Promise(resolve => setTimeout(resolve, 20));
      return { ok: true, status: 200, json: async () => ({ access_token: 'access-2', refresh_token: 'refresh-2', expires_in: 21_600 }) };
    };
    // Two attempts starting together share one exchange: the refresh token rotates.
    const [first, second] = await Promise.all([1, 2].map(() =>
      ensureFreshGrokLogin(path, 4.5 * HOUR, { now: () => NOW, fetch: exchange, waitMs: 5 })));
    assert.equal(requests.length, 1);
    assert.equal(requests[0]!.url, 'https://auth.x.ai/oauth2/token');
    assert.deepEqual(Object.fromEntries(new URLSearchParams(requests[0]!.body)),
      { grant_type: 'refresh_token', refresh_token: 'refresh-1', client_id: 'client-1' });
    assert.equal(first!.token, 'access-2');
    assert.equal(second!.token, 'access-2');
    const stored = JSON.parse(readFileSync(path, 'utf8'))[SCOPE];
    assert.deepEqual([stored.key, stored.refresh_token, stored.email, stored.expires_at],
      ['access-2', 'refresh-2', 'kept@example.com', new Date(NOW + 6 * HOUR).toISOString()]);
    if (process.platform !== 'win32') assert.equal(statSync(path).mode & 0o777, 0o600);
    assert.equal(first!.identity, before.identity, 'a campaign pins the account, not the rotating token');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a refused or short refresh leaves the stored login unchanged and asks for a new sign-in', async () => {
  const { root, path } = fixture(login(NOW + HOUR));
  try {
    const original = readFileSync(path, 'utf8');
    await assert.rejects(ensureFreshGrokLogin(path, 4.5 * HOUR, { now: () => NOW,
      fetch: async () => ({ ok: false, status: 400, json: async () => ({ error: 'invalid_grant' }) }) }), /grok login --device-auth/);
    assert.equal(readFileSync(path, 'utf8'), original);
    await assert.rejects(ensureFreshGrokLogin(path, 4.5 * HOUR, { now: () => NOW,
      fetch: async () => ({ ok: true, status: 200, json: async () => ({ access_token: 'short', expires_in: 600 }) }) }),
    /shorter than an attempt/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a lock left by a crashed refresher is reclaimed', async () => {
  const { root, path } = fixture(login(NOW + HOUR));
  try {
    mkdirSync(`${path}.stack-bench-lock`);
    const old = new Date(Date.now() - 10 * 60_000);
    utimesSync(`${path}.stack-bench-lock`, old, old);
    const result = await ensureFreshGrokLogin(path, 4.5 * HOUR, { now: () => NOW, waitMs: 5,
      fetch: async () => ({ ok: true, status: 200, json: async () => ({ access_token: 'access-3', expires_in: 21_600 }) }) });
    assert.equal(result.token, 'access-3');
    assert.equal(JSON.parse(readFileSync(path, 'utf8'))[SCOPE].refresh_token, 'refresh-1', 'an unrotated refresh token is kept');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('only a single auth.x.ai OIDC sign-in is accepted', () => {
  assert.throws(() => readGrokLogin('not json'), /JSON written by `grok login`/);
  assert.throws(() => readGrokLogin(JSON.stringify({})), /exactly one auth\.x\.ai sign-in/);
  assert.throws(() => readGrokLogin(JSON.stringify({ ...login(NOW), 'https://auth.x.ai::other': login(NOW)[SCOPE] })),
    /exactly one/);
  const legacy = login(NOW);
  delete (legacy[SCOPE] as Record<string, unknown>).refresh_token;
  assert.throws(() => readGrokLogin(JSON.stringify(legacy)), /not an auth\.x\.ai OIDC sign-in/);
});
