import { createSpacetimeGradingContext,
  spacetimeNamedActionRequest } from '../../actions/stack-action-operations.mjs';
import { activateSpacetime, controlSpacetime } from '../stack-lifecycle-operations.mjs';
import { noConnectionUrl, spacetimeBuildContainerPlan,
  spacetimeSetupMetadata } from '../stack-agent-operations.mjs';
import { deploySpacetimeReference } from '../stack-reference-operations.mjs';
import { spacetimeOrchestratorConfig } from '../stack-orchestrator-operations.mjs';
import { stopSpacetimeHost } from '../stack-teardown-operations.mjs';
import { stackLeaseCapability } from '../stack-lease-capabilities.mjs';
import { prepareSpacetimeDatabase, resetSpacetime, setSpacetimeStock } from './spacetime-operations.mjs';
import { SPACETIME_ADAPTER_VERSION } from './spacetime-identity.mjs';
import { defineStackAdapter, operationProvider, runPolicyProvider } from '../stack-adapter-common.mjs';

export const spacetimeAdapter = defineStackAdapter('spacetime', stackLeaseCapability('spacetime'), {
  reset: operationProvider('spacetime', 'reset',
    { run: resetSpacetime, 'requires-reseed': () => false }),
  'database-write': operationProvider('spacetime', 'database-write', { 'set-stock': setSpacetimeStock }),
  lifecycle: operationProvider('spacetime', 'lifecycle',
    { activate: activateSpacetime, control: controlSpacetime }),
  database: operationProvider('spacetime', 'database', { prepare: prepareSpacetimeDatabase }),
  grading: operationProvider('spacetime', 'grading', { context: createSpacetimeGradingContext }),
  'named-action': operationProvider('spacetime', 'named-action', { request: spacetimeNamedActionRequest }),
  teardown: operationProvider('spacetime', 'teardown', { host: stopSpacetimeHost }),
  'run-policy': runPolicyProvider('spacetime',
    { 'reset-enabled': true, 'sandbox-probe-required': true, 'product-review-enabled': true,
      'product-review-comparisons': ['postgres', 'mongodb'], 'retain-host-supported': true,
      'supervisor-env': ({ spacetimePort }) =>
        ({ STACK_BENCH_STDB_URI: `http://127.0.0.1:${spacetimePort}` }) }),
  agent: operationProvider('spacetime', 'agent', {
    'connection-url': noConnectionUrl,
    'minimal-guidance-supported': () => false,
    'default-skills': () => ['typescript-server', 'typescript-client'],
    'linux-cli-required': () => true,
    'setup-metadata': spacetimeSetupMetadata,
    'server-directory': () => 'backend',
    'find-database-urls': () => [],
  }),
  'build-container': operationProvider('spacetime', 'build-container', { plan: spacetimeBuildContainerPlan }),
  reference: operationProvider('spacetime', 'reference', { deploy: deploySpacetimeReference }),
  orchestrator: operationProvider('spacetime', 'orchestrator', { config: spacetimeOrchestratorConfig }),
}, { version: SPACETIME_ADAPTER_VERSION });
