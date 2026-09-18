import assert from 'node:assert/strict';
import test from 'node:test';
import { authenticationRealm, resetAuthenticationRealm } from '../src/runtime/authentication-service.js';
import { resolveGuidanceProfile } from '../src/campaigns/condition-compiler.js';

test('supplied identity credentials match the selected prompt guidance on every stack', () => {
  const original = ['stackbench-admin-2026', 'stackbench-staff-2026', 'stackbench-customer-2026'];
  for (const stack of ['spacetime', 'postgres', 'mongodb']) {
    for (const profile of ['prescribed', 'neutral-dev']) {
      const aliases = resolveGuidanceProfile(profile, [stack]).credentialAliases;
      const realm = authenticationRealm('http://127.0.0.1:6473/', aliases);
      assert.deepEqual(realm.users.map(user => user.credentials[0]!.value),
        original.map(password => aliases?.[password] ?? password), `${stack} / ${profile}`);
    }
  }
  assert.deepEqual(authenticationRealm('http://127.0.0.1:6473/').users.map(user => user.credentials[0]!.value),
    original, 'a build without a study condition retains original fixture credentials');
});

test('identity reset removes users and sessions without deleting the issuer or its keys', async () => {
  const calls: Array<{ path: string; method: string; body: unknown }> = [];
  let users = ['first', 'second'];
  const realm = authenticationRealm('http://127.0.0.1:6473/', { 'stackbench-admin-2026': 'alternate-password' });
  const fetchImpl: typeof fetch = async (url, init) => {
    const path = new URL(String(url)).pathname + new URL(String(url)).search;
    const method = init?.method ?? 'GET';
    const body = typeof init?.body === 'string' ? JSON.parse(init.body) : undefined;
    calls.push({ path, method, body });
    if (path.endsWith('/token')) return Response.json({ access_token: 'admin-token' });
    if (path === '/admin/realms/stack-bench') {
      assert.equal(method, 'GET', 'reset must preserve the issuer');
      return Response.json({ realm: 'stack-bench' });
    }
    if (path.endsWith('/users?first=0&max=100')) return Response.json(users.map(id => ({ id })));
    if (method === 'DELETE') users = users.filter(id => !path.endsWith(`/${id}`));
    if (path.endsWith('/users/profile') && method === 'GET') return Response.json({ attributes:
      ['username', 'email', 'firstName', 'lastName'].map(name => ({ name, required: { roles: ['user'] } })) });
    return new Response(null, { status: 204 });
  };
  await resetAuthenticationRealm({ password: 'private', realm }, fetchImpl);
  assert.deepEqual(users, []);
  assert(calls.some(call => call.path.endsWith('/logout-all') && call.method === 'POST'));
  assert.deepEqual(calls.find(call => call.path.endsWith('/partialImport'))?.body,
    { ifResourceExists: 'FAIL', users: realm.users });
  assert.equal(realm.users[0]!.credentials[0]!.value, 'alternate-password');
  assert.deepEqual(realm.roles.realm.map(role => role.name), ['app-admin', 'app-staff', 'app-customer']);
  assert.deepEqual(realm.users.map(user => user.realmRoles), [['app-admin'], ['app-staff'], ['app-customer']]);
  assert.equal(realm.clients[0]!.protocolMappers[0]!.config['id.token.claim'], 'true');
  const profile = calls.find(call => call.path.endsWith('/users/profile') && call.method === 'PUT')?.body as {
    attributes: Array<{ name: string; required?: unknown; permissions?: { edit: string[] } }>;
  };
  assert(profile.attributes.find(attribute => attribute.name === 'username')?.required);
  for (const attribute of profile.attributes.filter(attribute => attribute.name !== 'username')) {
    assert.equal(attribute.required, undefined);
    assert.deepEqual(attribute.permissions?.edit, ['admin']);
  }
});

test('identity administration refuses failed credentials and failed cleanup', async () => {
  const realm = authenticationRealm('http://127.0.0.1:6473/');
  await assert.rejects(resetAuthenticationRealm({ password: 'wrong', realm }, async () =>
    new Response(null, { status: 401 })), /credentials were refused/);
  let imported = false;
  await assert.rejects(resetAuthenticationRealm({ password: 'private', realm }, async (url) => {
    const path = new URL(String(url)).pathname;
    if (path.endsWith('/token')) return Response.json({ access_token: 'admin-token' });
    if (path.endsWith('/stack-bench')) return Response.json({ realm: 'stack-bench' });
    if (path.endsWith('/partialImport')) imported = true;
    return new Response(null, { status: 500 });
  }), /logout-all failed: 500/);
  assert.equal(imported, false);
});
