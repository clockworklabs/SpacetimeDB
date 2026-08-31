import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const readPack = (name: string) => compilePackDefinition(
  JSON.parse(readFileSync(join(packRoot, name), 'utf8')), { source: name });

test('staff activity and recommendation feedback own dedicated contracts', () => {
  const activity = readPack('progression-staff-activity-1.0.2.json');
  const feedback = readPack('progression-recommendation-feedback-2.0.0.json');
  for (const pack of [activity, feedback]) {
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.contracts.length, 1);
    for (const fragment of [...pack.task.requirements, ...pack.task.contracts]) {
      assert.deepEqual(fragment.modes, ['upgrade']);
      assert.equal(fragment.from, undefined);
      assert.equal(fragment.until, undefined);
    }
    const requirement = pack.task.requirements[0];
    assert.ok(requirement);
    const prompt = readFileSync(join(trackRoot, requirement.path), 'utf8');
    assert.doesNotMatch(prompt, /POST \/|reducer|framework|ORM|database|websocket/i);
  }
  const activityCheck = activity.checks[0];
  assert.ok(activityCheck);
  assert.equal(activityCheck.source, 'scenarios/progression-staff-activity-1.0.0.json');
  const scenario = compileScenarioDefinition(
    JSON.parse(readFileSync(join(trackRoot, activityCheck.source), 'utf8')),
    { source: activityCheck.source, expectedLevel: 5 });
  const feature = scenario.features[0];
  assert.ok(feature);
  const steps = [...feature.setup,
    ...feature.criteria.flatMap(criterion => criterion.steps)];
  assert(steps.some(step => step.testid === 'catalog-save'));
  assert(steps.some(step => step.testid === 'activity-time'));
  assert(!steps.some(step => step.testid === 'price-submit'));
});
