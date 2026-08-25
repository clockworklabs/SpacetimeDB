import assert from 'node:assert/strict';
import test from 'node:test';

import { completeAbortedHooks, completeUnvisitedHooks } from '../linter/lint.mjs';

test('contract lint fails closed when a golden path forgets a lintable stage', () => {
  const hooks = [
    { id: 'seen', stage: 'landing' },
    { id: 'forgotten', stage: 'operations' },
    { id: 'scenario-only', stage: 'scenario', note: 'requires two actors' },
  ];
  const results = [{ id: 'seen', status: 'PASS' }];

  completeUnvisitedHooks(hooks, results);

  assert.deepEqual(results, [
    { id: 'seen', status: 'PASS' },
    { id: 'forgotten', status: 'BLOCKED',
      detail: 'the golden path did not visit contract stage "operations"' },
    { id: 'scenario-only', status: 'SCENARIO', detail: 'requires two actors' },
  ]);
});

test('contract lint does not duplicate hooks already visited by the walk', () => {
  const hooks = [{ id: 'queue-depth', stage: 'fulfilment' }];
  const results = [{ id: 'queue-depth', status: 'FAIL', detail: 'missing' }];

  completeUnvisitedHooks(hooks, results);

  assert.equal(results.length, 1);
});

test('an unexpected walk error records one failure before blocking later hooks', () => {
  const hooks = [
    { id: 'seen', stage: 'landing' },
    { id: 'cart-panel', stage: 'cart' },
    { id: 'order-list', stage: 'after-checkout' },
    { id: 'scenario-only', stage: 'scenario', note: 'requires setup' },
  ];
  const results = [{ id: 'seen', status: 'PASS' }];

  completeAbortedHooks(hooks, results, new Error('target product was not visible\nlocator details'));

  assert.deepEqual(results, [
    { id: 'seen', status: 'PASS' },
    { id: 'cart-panel', status: 'FAIL',
      detail: 'golden path aborted before this hook: target product was not visible' },
    { id: 'order-list', status: 'BLOCKED', detail: 'golden path aborted' },
    { id: 'scenario-only', status: 'SCENARIO', detail: 'requires setup' },
  ]);
});
