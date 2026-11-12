import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const SCENARIO = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/01-open-list-live.json');

test('the focused 902a candidate deterministically checks an already-open live list', () => {
  const scenario = compileScenarioDefinition(readJson(SCENARIO),
    { source: SCENARIO, expectedLevel: 1 });
  const feature = scenario.features.find(candidate => candidate.id === 902);
  assert(feature, 'scenario must include feature 902');
  const criterion = feature.criteria.find(candidate => String(candidate.id) === '902a');
  assert(criterion, 'feature 902 must include criterion 902a');
  assert.equal(criterion.points, 1);
  assert.deepEqual(criterion.steps.map(step => step.do),
    ['openItem', 'expect', 'click', 'fill', 'click', 'expectElementCount', 'expectElementCount']);
  assert.deepEqual(criterion.steps.slice(0, 2).map(step => step.actor), ['reader', 'reader'],
    'the reader view must be visibly open before the write begins');
  assert.deepEqual(criterion.steps[2], {
    do: 'click', actor: 'reviewer', testid: 'review-toggle',
    unlessVisible: 'review-rating', ifAvailable: true,
  }, 'only the writer may open its review form; the reader must remain open');
  assert.equal(criterion.steps.some(action => action.do === 'race' || action.do === 'wait'), false);
  assert.deepEqual(criterion.steps.at(-2), {
    do: 'expectElementCount', actor: 'reviewer', testid: 'review-item',
    contains: 'live-review-kbd', equals: 1, within: 10000,
  }, 'the writer must prove the review committed before grading the reader');
  assert.deepEqual(criterion.steps.at(-1), {
    do: 'expectElementCount', actor: 'reader', testid: 'review-item',
    contains: 'live-review-kbd', equals: 1, within: 10000,
  });
  assert.match(stringValue(criterion.note, 'criterion 902a note'),
    /without assuming HTTP, WebSockets, subscriptions, or any project layout/);
});

function readJson(path: string): unknown {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function stringValue(value: unknown, label: string): string {
  if (typeof value !== 'string') throw new Error(`${label} must be a string`);
  return value;
}
