import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readPack = name => compilePackDefinition(
  JSON.parse(readFileSync(join(packRoot, name), 'utf8')), { source: name });

test('staff activity and recommendation feedback own dedicated contracts', () => {
  const activity = readPack('progression-staff-activity-1.0.0.json');
  const feedback = readPack('progression-recommendation-feedback-1.0.0.json');
  for (const pack of [activity, feedback]) {
    assert.equal(pack.state, 'draft');
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.contracts.length, 1);
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      assert.deepEqual(fragment.modes, ['upgrade']);
      assert.equal(fragment.from, undefined);
      assert.equal(fragment.until, undefined);
    }
    const prompt = readFileSync(join(trackRoot, pack.task.requirements[0].path), 'utf8');
    assert.doesNotMatch(prompt, /POST \/|reducer|framework|ORM|database|websocket/i);
  }
  assert.equal(activity.checks[0].source, 'scenarios/progression-staff-activity-1.0.0.json');
  const scenario = compileScenarioDefinition(
    JSON.parse(readFileSync(join(trackRoot, activity.checks[0].source), 'utf8')),
    { source: activity.checks[0].source, expectedLevel: 5 });
  const steps = [...scenario.features[0].setup,
    ...scenario.features[0].criteria.flatMap(criterion => criterion.steps)];
  assert(steps.some(step => step.testid === 'catalog-save'));
  assert(steps.some(step => step.testid === 'activity-time'));
  assert(!steps.some(step => step.testid === 'price-submit'));
});
