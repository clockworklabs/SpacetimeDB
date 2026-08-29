import assert from 'node:assert/strict';
import test from 'node:test';

import { completeAbortedHooks, completeUnvisitedHooks, selectHooks } from '../dist/linter/lint.mjs';
import { stableElementSelector } from '../dist/src/actions/element-selector.js';

test('stable element selectors support ordinary ids and existing test ids', () => {
  assert.equal(stableElementSelector('account-name'),
    '[data-testid="account-name"],#account-name');
  assert.throws(() => stableElementSelector('account name'), /invalid stable element id/);
});

test('a selected lint surface excludes unrelated hooks and keeps unknown hooks for scenario grading', () => {
  const selected = selectHooks([
    { id: 'accounts', stage: 'landing', check: 'visible' },
    { id: 'cart', stage: 'cart', check: 'visible' },
  ], ['support-link', 'accounts']);
  assert.deepEqual(selected.map(hook => hook.id), ['accounts', 'support-link']);
  assert.equal(selected[1].stage, 'scenario');
});

test('selected hooks keep contract order when one control reveals another', () => {
  const selected = selectHooks([
    { id: 'signin-toggle', stage: 'landing', check: 'visible' },
    { id: 'signin-username', stage: 'landing', check: 'attached', revealedBy: 'signin-toggle' },
    { id: 'signin-password', stage: 'landing', check: 'attached', revealedBy: 'signin-toggle' },
  ], ['signin-password', 'signin-toggle', 'signin-username']);

  assert.deepEqual(selected.map(hook => hook.id), [
    'signin-toggle',
    'signin-username',
    'signin-password',
  ]);
});

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
    { id: 'golden-path', status: 'FAIL',
      detail: 'golden path aborted: target product was not visible locator details' },
    { id: 'cart-panel', status: 'BLOCKED', detail: 'golden path aborted' },
    { id: 'order-list', status: 'BLOCKED', detail: 'golden path aborted' },
    { id: 'scenario-only', status: 'SCENARIO', detail: 'requires setup' },
  ]);
});
