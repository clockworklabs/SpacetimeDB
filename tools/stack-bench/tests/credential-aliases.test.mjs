import assert from 'node:assert/strict';
import test from 'node:test';

import { gradeArgv } from '../commands/bench.mjs';
import { agentRequestArgv } from '../src/agents/agent-adapter-contract.mjs';
import { resolveGuidanceProfile } from '../src/campaigns/condition-compiler.mjs';
import { agentVisibleContractText } from '../src/composition/agent-visible-contract.mjs';
import { materializeScenarioCredentials, validateCredentialAliases }
  from '../src/composition/credential-aliases.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';
import { seededAdminPassword } from '../tracks/ecommerce/walk.mjs';

const aliases = {
  'stackbench-admin-2026': 'store-admin-2026',
  'stackbench-customer-2026': 'store-customer-2026',
  'stackbench-staff-2026': 'store-staff-2026',
};

test('neutral 1.2 gives the agent and grader the same credential aliases', () => {
  const guidance = resolveGuidanceProfile('neutral@1.2.0', ['mongodb']);
  assert.deepEqual(guidance.credentialAliases, aliases);

  const source = 'Admin: `stackbench-admin-2026`; staff: `stackbench-staff-2026`; '
    + 'customer: `stackbench-customer-2026`.';
  const visible = agentVisibleContractText(source, guidance.credentialAliases);
  assert.equal(visible,
    'Admin: `store-admin-2026`; staff: `store-staff-2026`; customer: `store-customer-2026`.');

  const scenario = { features: [{ setup: [
    { do: 'signIn', password: 'stackbench-admin-2026' },
    { do: 'fill', text: 'stackbench-staff-2026' },
    { do: 'fill', text: 'ordinary text' },
  ] }] };
  const materialized = materializeScenarioCredentials(scenario, guidance.credentialAliases);
  assert.equal(materialized.features[0].setup[0].password, 'store-admin-2026');
  assert.equal(materialized.features[0].setup[1].text, 'store-staff-2026');
  assert.equal(materialized.features[0].setup[2].text, 'ordinary text');
  assert.equal(scenario.features[0].setup[0].password, 'stackbench-admin-2026');
});

test('campaign agent and grader argv carry aliases only when the condition declares them', () => {
  const adapter = { id: 'fake', entrypoint: 'agent.mjs', modes: ['build'], costLimit: 'non-billable' };
  const request = { mode: 'build', backend: 'mongodb', level: 1, app: '/app', track: 'ecommerce',
    runIndex: 0, model: 'fake', guidance: 'neutral', credentialAliases: aliases };
  const agentArgs = agentRequestArgv(adapter, request);
  assert.equal(agentArgs[agentArgs.indexOf('--credential-aliases-json') + 1], JSON.stringify(aliases));

  const args = { backend: 'mongodb', track: 'ecommerce', runIndex: 0, media: false,
    condition: { guidance: { credentialAliases: aliases } } };
  const track = loadTrack('ecommerce');
  const graderArgs = gradeArgv(args, '/app', 'http://app', 'attempt', 1,
    track, 'attempt');
  assert.equal(graderArgs[graderArgs.indexOf('--credential-aliases-json') + 1],
    JSON.stringify(aliases));

  delete request.credentialAliases;
  delete args.condition;
  assert.equal(agentRequestArgv(adapter, request).includes('--credential-aliases-json'), false);
  assert.equal(gradeArgv(args, '/app', 'http://app', 'attempt', 1,
    track, 'attempt').includes('--credential-aliases-json'), false);
});

test('model-free scenarios keep their original credentials without aliases', () => {
  const scenario = { features: [{ criteria: [{ actions: [
    { do: 'signIn', password: 'stackbench-admin-2026' },
    { do: 'fill', text: 'stackbench-customer-2026' },
  ] }] }] };
  assert.strictEqual(materializeScenarioCredentials(scenario), scenario);
  assert.equal(agentVisibleContractText('stackbench-staff-2026'), 'stackbench-staff-2026');
  assert.equal(seededAdminPassword(), 'stackbench-admin-2026');
});

test('the contract walk uses the same neutral admin credential', () => {
  assert.equal(seededAdminPassword(aliases), 'store-admin-2026');
});

test('credential aliases reject ambiguous or ineffective maps', () => {
  assert.throws(() => validateCredentialAliases({ same: 'same' }), /must change/);
  assert.throws(() => validateCredentialAliases({ first: 'shared', second: 'shared' }),
    /duplicated/);
  assert.throws(() => validateCredentialAliases([]), /must be an object/);
});
