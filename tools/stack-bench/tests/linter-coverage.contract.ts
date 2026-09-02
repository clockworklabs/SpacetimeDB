import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import { completeAbortedHooks, completeUnvisitedHooks, loadHooks, selectHooks } from '../linter/lint.js';
import { stableElementSelector } from '../src/actions/element-selector.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

interface TestHook {
  id: string;
  element: string;
  stage: string;
  check: 'visible' | 'attached';
  note: string;
  revealedBy?: string;
}

interface TestResult {
  id: string;
  status: 'PASS' | 'FAIL' | 'BLOCKED' | 'SCENARIO';
  detail?: string;
}

test('stable element selectors support ordinary ids and existing test ids', () => {
  assert.equal(stableElementSelector('account-name'),
    '[data-testid="account-name"],#account-name');
  assert.throws(() => stableElementSelector('account name'), /invalid stable element id/);
});

test('a selected lint surface excludes unrelated hooks and keeps unknown hooks for scenario grading', () => {
  const selected = selectHooks([
    { id: 'accounts', element: 'accounts', stage: 'landing', check: 'visible', note: '' },
    { id: 'cart', element: 'cart', stage: 'cart', check: 'visible', note: '' },
  ], ['support-link', 'accounts']);
  assert.deepEqual(selected.map(hook => hook.id), ['accounts', 'support-link']);
  assert.equal(selected[1]?.stage, 'scenario');
});

test('selected scenario controls do not require retired JSON contract files', () => {
  const hooks = loadHooks(3, { contracts: resolve(STACK_BENCH_ROOT, 'tracks/ecommerce/contracts') },
    ['current-user']);
  assert.deepEqual(hooks, [{
    id: 'current-user',
    element: 'the selected application control current-user',
    stage: 'scenario',
    check: 'visible',
    note: 'checked by the selected feature suite',
  }]);
});

test('selected hooks keep contract order when one control reveals another', () => {
  const selected = selectHooks([
    { id: 'signin-toggle', element: 'sign-in toggle', stage: 'landing', check: 'visible', note: '' },
    { id: 'signin-username', element: 'sign-in user', stage: 'landing', check: 'attached', note: '', revealedBy: 'signin-toggle' },
    { id: 'signin-password', element: 'sign-in password', stage: 'landing', check: 'attached', note: '', revealedBy: 'signin-toggle' },
  ], ['signin-password', 'signin-toggle', 'signin-username']);

  assert.deepEqual(selected.map(hook => hook.id), [
    'signin-toggle',
    'signin-username',
    'signin-password',
  ]);
});

test('contract lint fails closed when a core flow forgets a lintable stage', () => {
  const hooks: TestHook[] = [
    { id: 'seen', element: 'seen control', stage: 'landing', check: 'visible', note: '' },
    { id: 'forgotten', element: 'forgotten control', stage: 'operations', check: 'visible', note: '' },
    { id: 'scenario-only', element: 'scenario control', stage: 'scenario', check: 'visible', note: 'requires two actors' },
  ];
  const results: TestResult[] = [{ id: 'seen', status: 'PASS' }];

  completeUnvisitedHooks(hooks, results);

  assert.deepEqual(results, [
    { id: 'seen', status: 'PASS' },
    { id: 'forgotten', status: 'BLOCKED',
      detail: 'the core flow did not visit contract stage "operations"' },
    { id: 'scenario-only', status: 'SCENARIO', detail: 'requires two actors' },
  ]);
});

test('contract lint does not duplicate hooks already visited by the walk', () => {
  const hooks: TestHook[] = [{ id: 'queue-depth', element: 'queue depth', stage: 'fulfilment', check: 'visible', note: '' }];
  const results: TestResult[] = [{ id: 'queue-depth', status: 'FAIL', detail: 'missing' }];

  completeUnvisitedHooks(hooks, results);

  assert.equal(results.length, 1);
});

test('an unexpected walk error records one failure before blocking later hooks', () => {
  const hooks: TestHook[] = [
    { id: 'seen', element: 'seen control', stage: 'landing', check: 'visible', note: '' },
    { id: 'cart-panel', element: 'cart panel', stage: 'cart', check: 'visible', note: '' },
    { id: 'order-list', element: 'order list', stage: 'after-checkout', check: 'visible', note: '' },
    { id: 'scenario-only', element: 'scenario control', stage: 'scenario', check: 'visible', note: 'requires setup' },
  ];
  const results: TestResult[] = [{ id: 'seen', status: 'PASS' }];

  completeAbortedHooks(hooks, results, new Error('target product was not visible\nlocator details'));

  assert.deepEqual(results, [
    { id: 'seen', status: 'PASS' },
    { id: 'core-flow', status: 'FAIL',
      detail: 'core flow aborted: target product was not visible locator details' },
    { id: 'cart-panel', status: 'BLOCKED', detail: 'core flow aborted' },
    { id: 'order-list', status: 'BLOCKED', detail: 'core flow aborted' },
    { id: 'scenario-only', status: 'SCENARIO', detail: 'requires setup' },
  ]);
});
