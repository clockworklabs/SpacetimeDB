import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { ensureFreshAgyLogin, readAgyLogin } from '../src/agents/agy-login.js';

const MINUTE = 60_000;
const NOW = Date.parse('2026-10-09T12:00:00Z');

function login(expiresAt: number, access = 'ya29.access-1', subject = 'user-1') {
  const claims = Buffer.from(JSON.stringify({ iss: 'https://accounts.google.com', aud: 'client-1', sub: subject,
    email: 'kept@example.com' })).toString('base64url');
  return JSON.stringify({ token: { access_token: access, token_type: 'Bearer', refresh_token: '1//refresh-1',
    expiry: new Date(expiresAt).toISOString() }, auth_method: 'consumer', id_token: `header.${claims}.signature` });
}

async function withLogin(text: string, body: (path: string) => Promise<void>): Promise<void> {
  const root = mkdtempSync(join(tmpdir(), 'agy-login-'));
  const path = join(root, 'agy_auth');
  writeFileSync(path, `${text}\n`, { mode: 0o600 });
  try { await body(path); } finally { rmSync(root, { recursive: true, force: true }); }
}

test('an Antigravity login with enough time left is used as it is', async () => {
  await withLogin(login(NOW + 50 * MINUTE), async path => {
    const result = await ensureFreshAgyLogin(path, 40 * MINUTE,
      () => assert.fail('no renewal while enough time remains'), { now: () => NOW });
    assert.equal(result.token, 'ya29.access-1');
  });
});

test('an expiring Antigravity login is renewed by the CLI, written in place, and keeps its account identity', async () => {
  await withLogin(login(NOW + 10 * MINUTE), async path => {
    const before = readAgyLogin(readFileSync(path, 'utf8'));
    const renewals: string[] = [];
    const renew = async (text: string) => { renewals.push(text); return login(NOW + 60 * MINUTE, 'ya29.access-2'); };
    const results = await Promise.all([1, 2].map(() => ensureFreshAgyLogin(path, 40 * MINUTE, renew, { now: () => NOW, waitMs: 5 })));
    assert.equal(renewals.length, 1, 'concurrent sessions renew once');
    assert.deepEqual(results.map(result => result.token), ['ya29.access-2', 'ya29.access-2']);
    assert.equal(results[0]!.identity, before.identity);
    assert.equal(readAgyLogin(readFileSync(path, 'utf8')).token, 'ya29.access-2');
  });
});

test('a renewal that changes the account or does not extend the sign-in is refused and leaves the file', async () => {
  for (const renewed of [login(NOW + 60 * MINUTE, 'ya29.other', 'user-2'), login(NOW + 20 * MINUTE, 'ya29.short')]) {
    await withLogin(login(NOW + 10 * MINUTE), async path => {
      await assert.rejects(ensureFreshAgyLogin(path, 40 * MINUTE, async () => renewed, { now: () => NOW }),
        /sign in with agy again/);
      assert.equal(readAgyLogin(readFileSync(path, 'utf8')).token, 'ya29.access-1');
    });
  }
});

test('only a Google account sign-in from agy is accepted', () => {
  assert.throws(() => readAgyLogin('{"token":{}}'), /not a Google account sign-in/);
  assert.throws(() => readAgyLogin('not json'), /JSON file agy writes/);
  const other = JSON.parse(login(NOW));
  other.id_token = `h.${Buffer.from(JSON.stringify({ iss: 'https://example.com', sub: 'x' })).toString('base64url')}.s`;
  assert.throws(() => readAgyLogin(JSON.stringify(other)), /no Google account identity/);
});
