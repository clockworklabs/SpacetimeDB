import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';

import type { Page } from 'playwright';
import { type LintResult, checkHook, completeAbortedHooks, completeUnvisitedHooks, loadHooks, selectHooks } from '../linter/lint.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

interface TestHook {
  id: string;
  element: string;
  stage: string;
  check: 'visible' | 'attached';
  note: string;
  revealedBy?: string;
}

test('a selected lint surface excludes unrelated hooks and keeps unknown hooks for scenario grading', () => {
  const hooks: TestHook[] = [
    { id: 'accounts', element: 'accounts', stage: 'landing', check: 'visible', note: '' },
    { id: 'cart', element: 'cart', stage: 'cart', check: 'visible', note: '' },
  ];
  assert.deepEqual(selectHooks(hooks), hooks);
  assert.deepEqual(selectHooks(hooks, []), []);
  const selected = selectHooks(hooks, ['support-link', 'accounts']);
  assert.deepEqual(selected.map(hook => hook.id), ['accounts', 'support-link']);
  assert.equal(selected[1]?.stage, 'scenario');
  // Selected hooks keep contract order when one control reveals another.
  assert.deepEqual(selectHooks([
    { id: 'signin-toggle', element: 'sign-in toggle', stage: 'landing', check: 'visible', note: '' },
    { id: 'signin-username', element: 'sign-in user', stage: 'landing', check: 'attached', note: '', revealedBy: 'signin-toggle' },
    { id: 'signin-password', element: 'sign-in password', stage: 'landing', check: 'attached', note: '', revealedBy: 'signin-toggle' },
  ], ['signin-password', 'signin-toggle', 'signin-username']).map(hook => hook.id),
  ['signin-toggle', 'signin-username', 'signin-password']);
  // Selected scenario controls do not require retired JSON contract files.
  assert.deepEqual(loadHooks(3, { contracts: resolve(STACK_BENCH_ROOT, 'tracks/ecommerce/contracts') },
    ['current-user']), [{
    id: 'current-user',
    element: 'the selected application control current-user',
    stage: 'scenario',
    check: 'visible',
    note: 'checked by the selected feature suite',
  }]);
});

test('contract lint fails closed when a core flow forgets a lintable stage', () => {
  const hooks: TestHook[] = [
    { id: 'seen', element: 'seen control', stage: 'landing', check: 'visible', note: '' },
    { id: 'forgotten', element: 'forgotten control', stage: 'operations', check: 'visible', note: '' },
    { id: 'scenario-only', element: 'scenario control', stage: 'scenario', check: 'visible', note: 'requires two actors' },
  ];
  const results: LintResult[] = [{ id: 'seen', status: 'PASS' }];

  completeUnvisitedHooks(hooks, results);

  assert.deepEqual(results, [
    { id: 'seen', status: 'PASS' },
    { id: 'forgotten', status: 'BLOCKED',
      detail: 'the core flow did not visit contract stage "operations"' },
    { id: 'scenario-only', status: 'SCENARIO', detail: 'requires two actors' },
  ]);
});

test('an unexpected walk error records one failure before blocking later hooks', () => {
  const hooks: TestHook[] = [
    { id: 'seen', element: 'seen control', stage: 'landing', check: 'visible', note: '' },
    { id: 'cart-panel', element: 'cart panel', stage: 'cart', check: 'visible', note: '' },
    { id: 'order-list', element: 'order list', stage: 'after-checkout', check: 'visible', note: '' },
    { id: 'scenario-only', element: 'scenario control', stage: 'scenario', check: 'visible', note: 'requires setup' },
  ];
  const results: LintResult[] = [{ id: 'seen', status: 'PASS' }];

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

test('browser, script, and navigation faults in the core flow are not app failures', () => {
  const hooks: TestHook[] = [{ id: 'cart-panel', element: 'cart panel', stage: 'cart', check: 'visible', note: '' }];
  const coreFlow = (error: Error): string => {
    const results: LintResult[] = [];
    completeAbortedHooks(hooks, results, error);
    return results[0]!.status;
  };
  assert.equal(coreFlow(new Error('Target page, context or browser has been closed')), 'HARNESS');
  assert.equal(coreFlow(new TypeError('Cannot read properties of undefined')), 'HARNESS');
  assert.equal(coreFlow(new Error('page.goto: Timeout 15000ms exceeded.')), 'UNMEASURED');
});

test('a hook fails only when its control does not appear', async () => {
  const hook: TestHook = { id: 'cart-panel', element: 'cart panel', stage: 'cart', check: 'visible', note: '' };
  const page = (error: Error) => ({ locator: () => ({ first: () => ({
    count: async () => 1, waitFor: async () => { throw error; } }) }) }) as unknown as Page;
  const timeout = Object.assign(new Error('locator.waitFor: Timeout 5000ms exceeded.'), { name: 'TimeoutError' });
  for (const [error, status] of [[timeout, 'FAIL'],
    [new Error('Target page, context or browser has been closed'), 'HARNESS'],
    [new Error('Unexpected token "[" while parsing css selector'), 'HARNESS']] as const) {
    const results: LintResult[] = [];
    await checkHook(page(error), hook, results);
    assert.equal(results[0]!.status, status, error.message);
  }
});
