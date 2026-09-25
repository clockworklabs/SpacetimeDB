import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { bindBrowserRequest } from '../src/actions/named-action-runtime.js';
import type { Actor } from '../src/actions/actor-action-runtime.js';
import { proveSupabaseUse, supabaseAuthRequestPatch, supabaseNamedActionRequest,
  supabaseWriteEndpoints } from '../src/stacks/backends/supabase-operations.js';
import { SUPABASE_GATEWAY, SUPABASE_SECRETS, supabaseExec, supabaseLease,
  withSupabaseLeaseEnvironment } from './helpers/supabase-lease.js';

const restock = { id: 'restock', path: '/api/admin/restock', reducer: 'admin_restock', args: [0, 0, 1],
  params: [{ name: 'itemId', in: 'body' as const }, { name: 'warehouseId', in: 'body' as const },
    { name: 'quantity', in: 'body' as const }] };

test('Supabase named actions call public functions through the data API with the project key', () => {
  const lease = supabaseLease();
  const exec = supabaseExec(lease, () => { throw new Error('named actions never use privileged SQL'); });
  const request = supabaseNamedActionRequest({ action: restock, input: { values: { itemId: '42', warehouseId: 7, quantity: 2 } },
    url: 'http://127.0.0.1:5173', lease, exec })!;
  assert.equal(request.url, `${SUPABASE_GATEWAY}/rest/v1/rpc/admin_restock`);
  assert.equal(request.method, 'POST');
  assert.deepEqual(JSON.parse(request.body), { itemId: '42', warehouseId: 7, quantity: 2 });
  assert.deepEqual(request.headers, { apikey: SUPABASE_SECRETS.anonKey, Authorization: `Bearer ${SUPABASE_SECRETS.anonKey}` });
  assert.equal(request.missingNote, 'no database function public.admin_restock');
  // Positional declared defaults are named by their parameters; path parameters are ordinary arguments.
  const buy = { id: 'buy', path: '/api/items/:id/buy', reducer: 'buy_now', args: [0],
    params: [{ name: 'itemId', in: 'path' as const, placeholder: ':id' }] };
  assert.deepEqual(JSON.parse(supabaseNamedActionRequest({ action: buy, input: {}, lease, exec })!.body), { itemId: 0 });
  assert.deepEqual(JSON.parse(supabaseNamedActionRequest({ action: restock, input: { args: [1, 2, 3] }, lease, exec })!.body),
    { itemId: 1, warehouseId: 2, quantity: 3 });
  assert.throws(() => supabaseNamedActionRequest({ action: { ...restock, params: [] }, input: { args: [1] }, lease, exec }),
    (error: unknown) => (error as { code?: unknown }).code === 'invalid_named_action_input');
  assert.equal(supabaseNamedActionRequest({ action: { id: 'ship', path: '/api/ship' }, lease, exec }), null);
  assert.equal(supabaseNamedActionRequest({ action: { ...restock, reducer: 'a/b' }, input: {}, lease, exec })!.url,
    `${SUPABASE_GATEWAY}/rest/v1/rpc/a%2Fb`);
});

test('platform headers go with every named call and the caller credential replaces the anonymous bearer', () => {
  const lease = supabaseLease();
  const request = supabaseNamedActionRequest({ action: restock, input: {}, lease,
    exec: supabaseExec(lease, () => '') })!;
  const actor = { name: 'staff', page: {}, writes: [] } as unknown as Actor;
  const signedIn = bindBrowserRequest(actor, request, { Authorization: 'Bearer staff-token' })();
  assert.deepEqual(signedIn.headers, { apikey: SUPABASE_SECRETS.anonKey, Authorization: 'Bearer staff-token' });
  // A differently cased caller header replaces the platform default instead of joining it.
  const lower = bindBrowserRequest(actor, request, {})({ 'Content-Type': 'application/json', authorization: 'Bearer x' });
  assert.deepEqual(lower.headers, { apikey: SUPABASE_SECRETS.anonKey, 'Content-Type': 'application/json',
    authorization: 'Bearer x' });
  const anonymous = bindBrowserRequest(actor, request, {})({ 'Content-Type': 'application/json' });
  assert.equal(anonymous.headers.Authorization, `Bearer ${SUPABASE_SECRETS.anonKey}`);
  assert.equal(anonymous.body, request.body);
  // Requests without platform headers are unchanged.
  assert.deepEqual(bindBrowserRequest(actor, { url: 'http://app.test/api', body: '{}' }, { Cookie: 'sid=1' })(),
    { headers: { Cookie: 'sid=1' }, body: '{}' });
});

test('named actions resolve the gateway and project key from the authenticated lease', t => {
  const lease = supabaseLease();
  withSupabaseLeaseEnvironment(t, lease);
  const calls: { args: readonly string[] }[] = [];
  const request = supabaseNamedActionRequest({ action: restock, input: {}, exec: supabaseExec(lease, () => '', calls) })!;
  assert.equal(request.url, `${SUPABASE_GATEWAY}/rest/v1/rpc/admin_restock`);
  assert.deepEqual(calls.map(call => call.args[0]), ['inspect', 'exec'], 'the anchor is verified before its key is read');
});

test('only data API and Edge Function requests to the leased gateway are application writes', () => {
  const lease = supabaseLease();
  const endpoints = supabaseWriteEndpoints(lease);
  const write = (url: string) => endpoints.some(endpoint => url.startsWith(endpoint));
  assert(write(`${SUPABASE_GATEWAY}/rest/v1/rpc/add_to_cart`));
  assert(write(`${SUPABASE_GATEWAY}/rest/v1/orders?select=*`));
  assert(write(`${SUPABASE_GATEWAY}/functions/v1/checkout`));
  for (const url of [`${SUPABASE_GATEWAY}/auth/v1/token?grant_type=password`, `${SUPABASE_GATEWAY}/storage/v1/object`,
    'http://127.0.0.1:5173/rest/v1/rpc/add_to_cart', 'http://127.0.0.1:13411/rest/v1/rpc/x']) {
    assert(!write(url), url);
  }
});

test('Supabase Auth password requests are found by endpoint whatever credential encoding the application sends', () => {
  const patch = supabaseAuthRequestPatch(supabaseLease());
  // The reference sends a hex-encoded username email and a digest of the typed password.
  const email = `${Buffer.from('claimant').toString('hex')}@accounts.invalid`;
  const digest = createHash('sha256').update('secret').digest('hex');
  const signUp = { email, password: digest, data: { username: 'claimant' }, gotrue_meta_security: {} };
  const fields = JSON.parse('{"role":"admin","__proto__":{"polluted":true}}');
  const changed = patch(`${SUPABASE_GATEWAY}/auth/v1/signup`, signUp, { fields })!;
  assert.equal(changed.shape, 'supabase-auth');
  assert.deepEqual(JSON.parse(changed.body), JSON.parse(JSON.stringify({ email, password: digest }).slice(0, -1)
    + ',"data":{"username":"claimant","role":"admin","__proto__":{"polluted":true}},"gotrue_meta_security":{},'
    + '"role":"admin","__proto__":{"polluted":true}}'));
  assert.equal(({} as { polluted?: unknown }).polluted, undefined);
  assert.deepEqual(signUp.data, { username: 'claimant' }, 'the application request is not modified in place');
  const signIn = { email, password: digest, gotrue_meta_security: {} };
  const token = `${SUPABASE_GATEWAY}/auth/v1/token?grant_type=password`;
  assert.deepEqual(JSON.parse(patch(token, signIn, { fields: { role: 'admin' } })!.body),
    { ...signIn, role: 'admin', data: { role: 'admin' } });
  // A query-like password replaces whatever the application derived from the typed one.
  assert.deepEqual(JSON.parse(patch(token, signIn, { password: "' OR '1'='1" })!.body),
    { ...signIn, password: "' OR '1'='1" }, 'a password change adds no metadata');
  // The reference now sends the typed password itself, up to 72 UTF-8 bytes.
  const typed = `${'界'.repeat(23)}x-A`;
  assert.equal(Buffer.byteLength(typed), 72);
  const raw = { email, password: typed, gotrue_meta_security: {} };
  assert.deepEqual(JSON.parse(patch(`${SUPABASE_GATEWAY}/auth/v1/signup`, raw, { fields: { role: 'admin' } })!.body),
    { ...raw, role: 'admin', data: { role: 'admin' } });
  assert.deepEqual(JSON.parse(patch(token, raw, { password: "' OR '1'='1" })!.body), { ...raw, password: "' OR '1'='1" });
  for (const url of [`${SUPABASE_GATEWAY}/auth/v1/token?grant_type=refresh_token`, `${SUPABASE_GATEWAY}/rest/v1/rpc/signup`,
    'http://127.0.0.1:5173/auth/v1/signup']) {
    assert.equal(patch(url, signUp, { fields }), undefined, url);
  }
  for (const body of [{ email }, { ...signIn, password: 7 }, ['secret'], null]) {
    assert.equal(patch(token, body, { fields }), null, JSON.stringify(body));
  }
  assert.throws(() => patch(token, { ...signUp, data: ['x'] }, { fields }), /metadata is not an object/);
});

test('Supabase provenance scans public application columns through privileged SQL', () => {
  const lease = supabaseLease();
  for (const [output, matches] of [['', 0], ['public.order_account.username\n', 1],
    ['public.order_account.username\npublic.profile.name\n', 2]] as const) {
    let sql = '';
    const result = proveSupabaseUse({ lease, marker: "sb'marker",
      exec: supabaseExec(lease, input => { sql = input; return output; }) });
    assert.equal(result.matches, matches);
    assert.equal(result.ok, matches > 0);
    assert.equal(result.verified, true);
    assert.match(sql, /WHERE table_schema = 'public'/);
    assert.match(sql, /'sb''marker'/);
    assert.match(sql, /\n\\gexec\n$/);
  }
  assert.throws(() => proveSupabaseUse({ lease, marker: '', exec: supabaseExec(lease, () => '') }), /non-empty/);
});
