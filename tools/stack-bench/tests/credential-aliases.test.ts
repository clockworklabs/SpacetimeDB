import assert from 'node:assert/strict';
import test from 'node:test';

import { gradeArgv } from '../commands/bench.js';
import { agentRequestArgv } from '../src/agents/agent-adapter-contract.js';
import { AGENT_ADAPTER_REGISTRY } from '../src/agents/agent-adapters.js';
import type { AgentRequest } from '../src/agents/agent-adapter-contract.js';
import { resolveGuidanceProfile } from '../src/campaigns/condition-compiler.js';
import { agentVisibleContractText } from '../src/composition/agent-visible-contract.js';
import { applyCredentialAliases, materializeScenarioCredentials, validateCredentialAliases }
  from '../src/composition/credential-aliases.js';
import { loadTrack } from '../src/composition/tracks.js';
import { seededAdminPassword } from '../tracks/ecommerce/walk.js';

const aliases = {
  'stackbench-admin-2026': 'store-admin-2026',
  'stackbench-customer-2026': 'store-customer-2026',
  'stackbench-staff-2026': 'store-staff-2026',
};

test('neutral guidance gives the agent and grader the same credential aliases', () => {
  const guidance = resolveGuidanceProfile('neutral@1.8.0', ['mongodb']);
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
  const feature = materialized.features[0];
  const originalFeature = scenario.features[0];
  assert(feature);
  assert(originalFeature);
  const [signIn, staff, ordinary] = feature.setup;
  const [originalSignIn] = originalFeature.setup;
  assert(signIn);
  assert(staff);
  assert(ordinary);
  assert(originalSignIn);
  assert.equal(signIn.password, 'store-admin-2026');
  assert.equal(staff.text, 'store-staff-2026');
  assert.equal(ordinary.text, 'ordinary text');
  assert.equal(originalSignIn.password, 'stackbench-admin-2026');
});

test('campaign agent and grader argv carry aliases only when the condition declares them', () => {
  const adapter = AGENT_ADAPTER_REGISTRY.get('deterministic');
  const request: AgentRequest = { mode: 'build', backend: 'mongodb', level: 1, app: '/app', track: 'ecommerce',
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

  const unaliasedRequest: AgentRequest = { ...request, credentialAliases: null };
  const unaliasedArgs = { ...args, condition: undefined };
  assert.equal(agentRequestArgv(adapter, unaliasedRequest).includes('--credential-aliases-json'), false);
  assert.equal(gradeArgv(unaliasedArgs, '/app', 'http://app', 'attempt', 1,
    track, 'attempt').includes('--credential-aliases-json'), false);
});

test('model-free scenarios keep their original credentials without aliases', () => {
  const scenario = { features: [{ criteria: [{ actions: [
    { do: 'signIn', password: 'stackbench-admin-2026' },
    { do: 'fill', text: 'stackbench-customer-2026' },
  ] }] }] };
  assert.strictEqual(materializeScenarioCredentials(scenario), scenario);
  assert.throws(() => agentVisibleContractText('stackbench-staff-2026'),
    /contains internal language/);
  assert.equal(seededAdminPassword({}), 'stackbench-admin-2026');
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

test('credential aliases apply once without cascading', () => {
  assert.equal(applyCredentialAliases('first second', { first: 'second', second: 'third' }),
    'second third');
});
