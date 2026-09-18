import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('password diagnostic exercises a suffix beyond bcrypt truncation within the account contract', () => {
  const input = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/diagnostic-password-boundary.json'), 'utf8'));
  const scenario = compileScenarioDefinition(input);
  assert.equal(scenario.features.length, 1);
  assert(scenario.features[0]!.criteria.every(check => check.points === 0));
  const steps = input.features[0].criteria[0].steps;
  const signup = steps.find((step: { do: string }) => step.do === 'signUp');
  const logins = steps.filter((step: { do: string }) => step.do === 'signIn');
  assert.equal(logins.length, 2);
  assert.equal(logins[0].password, signup.password);
  assert.notEqual(logins[0].actor, signup.actor); // Positive login must use a fresh session.
  assert.notEqual(logins[1].password, signup.password);
  assert.equal(logins[1].expectFailure, true);
  for (const step of [signup, ...logins]) {
    assert(step.password.length <= 64);
    assert(Buffer.byteLength(step.password, 'utf8') > 72);
    assert.deepEqual(Buffer.from(step.password).subarray(0, 72),
      Buffer.from(signup.password).subarray(0, 72));
  }
});
