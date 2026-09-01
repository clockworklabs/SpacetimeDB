import assert from 'node:assert/strict';
import test from 'node:test';

import {
  httpNamedActionRequest,
  spacetimeNamedActionRequest,
} from '../src/stacks/stack-grading-operations.js';

test('SpacetimeDB requests preserve full unsigned 64-bit values', () => {
  const request = spacetimeNamedActionRequest({
    action: {
      path: '',
      reducer: 'set_stock',
      params: [{ name: 'quantity', wireType: 'u64' }],
    },
    input: { values: { quantity: '18446744073709551615' } },
    spacetime: { uri: 'http://127.0.0.1:3000', mod: 'shop' },
  });

  assert.equal(request.url, 'http://127.0.0.1:3000/v1/database/shop/call/set_stock');
  assert.equal(request.body, '[18446744073709551615]');
  assert.throws(
    () => spacetimeNamedActionRequest({
      action: {
        path: '',
        reducer: 'set_stock',
        params: [{ name: 'quantity', wireType: 'u64' }],
      },
      input: { values: { quantity: '18446744073709551616' } },
      spacetime: { uri: 'http://127.0.0.1:3000', mod: 'shop' },
    }),
    (error: unknown) => {
      assert(error instanceof Error);
      assert.equal((error as Error & { code?: string }).code, 'invalid_named_action_input');
      return true;
    },
  );
});

test('HTTP requests substitute path values and keep other values in the body', () => {
  const request = httpNamedActionRequest({
    action: {
      path: '/items/:item',
      method: 'PATCH',
      params: [
        { name: 'item', in: 'path', placeholder: ':item' },
        { name: 'quantity' },
      ],
    },
    input: { values: { item: 'camera/one', quantity: 3 } },
    url: 'http://127.0.0.1:4000/',
  });

  assert.deepEqual(request, {
    url: 'http://127.0.0.1:4000/items/camera%2Fone',
    method: 'PATCH',
    body: '{"quantity":3}',
    missingNote: 'no route at /items/:item',
  });
});
