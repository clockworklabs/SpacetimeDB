import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { AUTH_CLIENT, AUTH_ISSUER, providerUsername } from '../reference-apps/ecommerce/spacetime/backend/spacetimedb/src/auth-policy.js';

test('provider accounts require the intended issuer, audience and reserved-account role', () => {
  const jwt = { issuer: AUTH_ISSUER, audience: [AUTH_CLIENT], fullPayload: { preferred_username: 'shopper-1' } };
  assert.equal(providerUsername(jwt), 'shopper-1');
  assert.throws(() => providerUsername({ ...jwt, issuer: 'https://untrusted.example' }), /issuer/);
  assert.throws(() => providerUsername({ ...jwt, audience: ['other-app'] }), /audience/);
  for (const username of ['admin', 'staff', 'customer']) {
    assert.throws(() => providerUsername({ ...jwt, fullPayload: { preferred_username: username } }), /role/);
    assert.throws(() => providerUsername({ ...jwt, fullPayload: {
      preferred_username: username, realm_access: { roles: ['unrelated-role'] },
    } }), /role/);
    assert.equal(providerUsername({ ...jwt, fullPayload: {
      preferred_username: username, realm_access: { roles: [`app-${username}`] },
    } }), username);
  }
  assert.throws(() => providerUsername({ ...jwt, fullPayload: { preferred_username: '<script>' } }), /username/);
});

test('reference stores identity bindings without password reducers or shared logout deletion', () => {
  const root = join(STACK_BENCH_ROOT, 'reference-apps/ecommerce/spacetime');
  const backend = readFileSync(join(root, 'backend/spacetimedb/src/index.ts'), 'utf8');
  const schema = readFileSync(join(root, 'backend/spacetimedb/src/schema.ts'), 'utf8');
  assert.doesNotMatch(backend, /hashPassword|export const sign(?:Up|In|Out)\b/);
  assert.doesNotMatch(schema, /passwordHash/);
  assert.doesNotMatch(backend, /session\.identity\.(?:delete|update)/);
  assert.match(backend, /ctx\.db\.session\.accountId\.find\(acc\.id\)/);
  assert.match(backend, /function requireAccount\(ctx: Ctx\) \{\s+requireProvider\(ctx\)/);
});
