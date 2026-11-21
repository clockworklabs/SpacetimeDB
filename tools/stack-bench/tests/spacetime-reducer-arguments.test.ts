import assert from 'node:assert/strict';
import test from 'node:test';

import { spacetimeNamedActionFetch, withOptionalReducerArguments }
  from '../src/stacks/backends/spacetime-reducer-arguments.js';

const target = { uri: 'http://127.0.0.1:3000', mod: 'shop' };
const schema = { typespace: { types: [{ Array: { String: [] } }] }, reducers: [
  { name: 'checkout', params: { elements: [{ algebraic_type: { String: [] } }, { algebraic_type: { Bool: [] } },
    { algebraic_type: { Sum: { variants: [{ name: { some: 'some' }, algebraic_type: { String: [] } },
      { name: { some: 'none' }, algebraic_type: { Product: { elements: [] } } }] } } },
    { algebraic_type: { Ref: 0 } }] } },
  { name: 'checkout_count', params: { elements: [{ algebraic_type: { U32: [] } }] } },
  { name: 'plain', params: { elements: [] } },
] };

function stub() {
  const calls: { url: string; body?: string | null }[] = [];
  const fetchImpl = async (url: string, options: { body?: string | null }) => {
    calls.push({ url, body: options.body });
    return { ok: true, status: 200, text: async () => JSON.stringify(schema) };
  };
  return { calls, fetchImpl };
}

test('a reducer called without arguments receives the blank value of each declared parameter', async () => {
  const { calls, fetchImpl } = stub();
  assert.equal(await withOptionalReducerArguments(target, 'checkout', '[]', fetchImpl),
    JSON.stringify(['', false, { none: [] }, []]));
  assert.equal(calls[0]!.url, 'http://127.0.0.1:3000/v1/database/shop/schema?version=9');
  // No blank value exists for a number, and a no-parameter reducer needs none.
  assert.equal(await withOptionalReducerArguments(target, 'checkout_count', '[]', fetchImpl), '[]');
  assert.equal(await withOptionalReducerArguments(target, 'plain', '[]', fetchImpl), '[]');
  // Given arguments are the interface's own; they are never looked up or changed.
  const before = calls.length;
  assert.equal(await withOptionalReducerArguments(target, 'checkout', '["Ada",true]', fetchImpl), '["Ada",true]');
  assert.equal(calls.length, before);
});

test('only reducer calls on the leased database are completed', async () => {
  const { calls, fetchImpl } = stub();
  const send = spacetimeNamedActionFetch(fetchImpl, target);
  await send('http://127.0.0.1:3000/v1/database/shop/call/checkout', { method: 'POST', body: '[]' });
  assert.equal(calls.at(-1)!.body, JSON.stringify(['', false, { none: [] }, []]));
  await send('http://127.0.0.1:3000/v1/database/other/call/checkout', { method: 'POST', body: '[]' });
  assert.equal(calls.at(-1)!.body, '[]');
  assert.equal(spacetimeNamedActionFetch(fetchImpl, null), fetchImpl);
});
