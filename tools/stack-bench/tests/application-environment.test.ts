import assert from 'node:assert/strict';
import test from 'node:test';
import { APPLICATION_ENVIRONMENT_VARIABLE, adoptRecordedApplicationEnvironment, applicationEnvironmentRule,
  applicationPortEnvironment, recordedApplicationEnvironmentRule, selectApplicationEnvironment }
  from '../src/stacks/application-environment.js';
import { validateCampaignExtensionSeed } from '../src/campaigns/campaign-scheduler.js';

test('new runs get neutral names; saved work keeps the names its run recorded', () => {
  assert.equal(applicationEnvironmentRule({}), 'neutral');
  assert.throws(() => applicationEnvironmentRule({ [APPLICATION_ENVIRONMENT_VARIABLE]: 'vite' }));
  assert.equal(recordedApplicationEnvironmentRule({ applicationEnvironment: 'neutral' }), 'neutral');
  // A run recorded before the rule existed gave its store the Vite-named copies.
  assert.equal(recordedApplicationEnvironmentRule({ tools: {} }), 'vite-names');
  assert.equal(recordedApplicationEnvironmentRule(undefined), 'vite-names');
  assert.throws(() => recordedApplicationEnvironmentRule({ applicationEnvironment: 'other' }));

  const sets = { neutral: { SERVICE_URL: 'u' }, 'vite-names': { SERVICE_URL: 'u', VITE_SERVICE_URL: 'u' } };
  assert.deepEqual(selectApplicationEnvironment(sets, 'neutral'), { SERVICE_URL: 'u' });
  assert.deepEqual(selectApplicationEnvironment(sets, 'vite-names'), { SERVICE_URL: 'u', VITE_SERVICE_URL: 'u' });
  assert.deepEqual(applicationPortEnvironment(6478, 'neutral'), { APP_PORT: '6478' });
  assert.deepEqual(applicationPortEnvironment(6478, 'vite-names'), { VITE_PORT: '6478' });

  const prior = process.env[APPLICATION_ENVIRONMENT_VARIABLE];
  try {
    assert.equal(adoptRecordedApplicationEnvironment({}), 'vite-names');
    assert.equal(applicationEnvironmentRule(), 'vite-names');
    assert.deepEqual(applicationPortEnvironment(1), { VITE_PORT: '1' });
    assert.equal(adoptRecordedApplicationEnvironment({ applicationEnvironment: 'neutral' }), 'neutral');
    assert.deepEqual(applicationPortEnvironment(1), { APP_PORT: '1' });
  } finally {
    if (prior === undefined) delete process.env[APPLICATION_ENVIRONMENT_VARIABLE];
    else process.env[APPLICATION_ENVIRONMENT_VARIABLE] = prior;
  }
});

test('an extension seed carries its parent run rule and older seeds stay valid', () => {
  const seed = { fromDepth: 1, source: '.private/extensions/a/source', sourceSha256: 'a'.repeat(64), sourceFiles: 3,
    parent: { campaignId: 'c', campaignSha256: 'b'.repeat(64), attemptId: 'a', executionId: 'e', runId: 'r',
      runSha256: 'c'.repeat(64) } };
  assert.equal(validateCampaignExtensionSeed(seed).applicationEnvironment, undefined);
  assert.equal(validateCampaignExtensionSeed({ ...seed, applicationEnvironment: 'neutral' }).applicationEnvironment,
    'neutral');
  assert.throws(() => validateCampaignExtensionSeed({ ...seed, applicationEnvironment: 'vite' }));
});
