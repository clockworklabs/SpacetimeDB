import { createHttpGradingContext, httpNamedActionRequest } from '../../actions/stack-action-operations.js';
import { activateHosted } from '../stack-lifecycle-operations.mjs';
import { emptySetupMetadata, noConnectionUrl,
  standardBuildContainerPlan } from '../stack-agent-operations.mjs';
import { standardOrchestratorConfig } from '../stack-orchestrator-operations.mjs';
import { stopHostedHost } from '../stack-teardown-operations.mjs';
import { stackLeaseCapability } from '../stack-lease-capabilities.mjs';
import { STUB_ADAPTER_VERSION } from './stub-identity.mjs';
import { defineStackAdapter, operationProvider, runPolicyProvider } from '../stack-adapter-common.mjs';

export const stubAdapter = defineStackAdapter('stub', stackLeaseCapability('stub'), {
  reset: operationProvider('stub', 'reset', { 'requires-reseed': () => false }),
  lifecycle: operationProvider('stub', 'lifecycle', { activate: activateHosted }),
  database: operationProvider('stub', 'database', { prepare: ({ name }) => name }),
  grading: operationProvider('stub', 'grading', { context: createHttpGradingContext }),
  'named-action': operationProvider('stub', 'named-action', { request: httpNamedActionRequest }),
  teardown: operationProvider('stub', 'teardown', { host: stopHostedHost }),
  'run-policy': runPolicyProvider('stub',
    { 'reset-enabled': false, 'sandbox-probe-required': false, 'product-review-enabled': false,
      'product-review-comparisons': [], 'retain-host-supported': false, 'supervisor-env': () => ({}) }),
  admission: operationProvider('stub', 'admission', {
    requirements: () => ({ docker: false, services: false, ports: false,
      credentials: false, providerAccess: false }),
  }),
  agent: operationProvider('stub', 'agent', {
    'connection-url': noConnectionUrl,
    'minimal-guidance-supported': () => true,
    'default-skills': () => [],
    'linux-cli-required': () => false,
    'setup-metadata': emptySetupMetadata,
    'server-directory': () => 'server',
    'find-database-urls': () => [],
  }),
  'build-container': operationProvider('stub', 'build-container',
    { plan: () => standardBuildContainerPlan() }),
  orchestrator: operationProvider('stub', 'orchestrator', { config: standardOrchestratorConfig }),
}, { version: STUB_ADAPTER_VERSION });
