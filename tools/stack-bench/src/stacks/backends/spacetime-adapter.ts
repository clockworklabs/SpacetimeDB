import { createSpacetimeGradingContext,
  spacetimeNamedActionRequest } from '../../actions/stack-action-operations.js';
import { activateSpacetime, controlSpacetime } from '../stack-lifecycle-operations.js';
import { noConnectionUrl, spacetimeBuildContainerPlan,
  spacetimeSetupMetadata } from '../stack-agent-operations.js';
import { deploySpacetimeReference } from '../stack-reference-operations.js';
import { spacetimeOrchestratorConfig } from '../stack-orchestrator-operations.js';
import { stopSpacetimeHost } from '../stack-teardown-operations.js';
import { stackLeaseCapability } from '../stack-lease-capabilities.js';
import { prepareSpacetimeDatabase, resetSpacetime, setSpacetimeStock } from './spacetime-operations.js';
import { SPACETIME_ADAPTER_VERSION } from './spacetime-identity.js';
import { defineStackAdapter, operationProvider, runPolicyProvider } from '../stack-adapter-common.js';

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
      'supervisor-env': ({ spacetimePort }: { spacetimePort: number }) =>
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
