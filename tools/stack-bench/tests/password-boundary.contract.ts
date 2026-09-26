import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('existing password check requires the full password and a valid fresh login within the account contract', () => {
  const input = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/01-account-password.json'), 'utf8'));
  const scenario = compileScenarioDefinition(input);
  assert.equal(scenario.features.length, 1);
  assert.equal(scenario.features[0]!.criteria.length, 1);
  assert.equal(scenario.features[0]!.criteria[0]!.id, '1c');
  assert.equal(scenario.features[0]!.criteria[0]!.points, 1);
  const steps = input.features[0].criteria[0].steps;
  const signup = steps.find((step: { do: string }) => step.do === 'signUp');
  const logins = steps.filter((step: { do: string }) => step.do === 'signIn');
  assert.equal(logins.length, 2);
  assert.equal(logins[0].password, signup.password);
  assert.notEqual(logins[0].actor, signup.actor); // Positive login must use a fresh session.
  assert.notEqual(logins[1].password, signup.password);
  assert.equal(logins[1].expectFailure, true);
  assert.notEqual(logins[1].actor, logins[0].actor);
  for (const step of [signup, ...logins]) {
    assert(step.password.length <= 64);
    assert(Buffer.byteLength(step.password, 'utf8') > 72);
    assert.deepEqual(Buffer.from(step.password).subarray(0, 72),
      Buffer.from(signup.password).subarray(0, 72));
  }
  const legacy = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/01-features.json'), 'utf8'));
  assert.deepEqual(legacy.features[0].criteria.find((check: { id: string }) => check.id === '1c').steps,
    steps.map((step: { actor: string }) => ({ ...step, actor: `password-${step.actor}` })));
});
