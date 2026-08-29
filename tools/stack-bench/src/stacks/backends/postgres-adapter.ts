import { createHttpGradingContext, httpNamedActionRequest } from '../../actions/stack-action-operations.js';
import { captureHostedDiagnostics, activateHosted } from '../stack-lifecycle-operations.mjs';
import { postgresConnectionUrl, postgresSetupMetadata,
  standardBuildContainerPlan } from '../stack-agent-operations.mjs';
import { deployPostgresReference } from '../stack-reference-operations.mjs';
import { standardOrchestratorConfig } from '../stack-orchestrator-operations.mjs';
import { stopHostedHost } from '../stack-teardown-operations.mjs';
import { stackLeaseCapability } from '../stack-lease-capabilities.mjs';
import { preparePostgresDatabase, provePostgresUse, resetPostgres,
  setPostgresStock } from './postgres-operations.mjs';
import { POSTGRES_ADAPTER_VERSION } from './postgres-identity.js';
import { controlHostedFor, defineStackAdapter, operationProvider,
  runPolicyProvider } from '../stack-adapter-common.js';
import type { StackAdapter } from '../stack-adapter-contract.mjs';

let postgresAdapter: StackAdapter;
postgresAdapter = defineStackAdapter('postgres', stackLeaseCapability('postgres'), {
  reset: operationProvider('postgres', 'reset',
    { run: resetPostgres, 'requires-reseed': () => true }),
  'database-write': operationProvider('postgres', 'database-write', { 'set-stock': setPostgresStock }),
  lifecycle: operationProvider('postgres', 'lifecycle',
    { activate: activateHosted, control: input => controlHostedFor(postgresAdapter, input) }),
  diagnostics: operationProvider('postgres', 'diagnostics', { capture: captureHostedDiagnostics }),
  database: operationProvider('postgres', 'database',
    { prepare: preparePostgresDatabase, 'prove-use': provePostgresUse }),
  grading: operationProvider('postgres', 'grading', { context: createHttpGradingContext }),
  'named-action': operationProvider('postgres', 'named-action', { request: httpNamedActionRequest }),
  teardown: operationProvider('postgres', 'teardown', { host: stopHostedHost }),
  'run-policy': runPolicyProvider('postgres',
    { 'reset-enabled': true, 'sandbox-probe-required': true, 'product-review-enabled': false,
      'product-review-comparisons': [], 'retain-host-supported': false, 'supervisor-env': () => ({}) }),
  agent: operationProvider('postgres', 'agent', {
    'connection-url': postgresConnectionUrl,
    'minimal-guidance-supported': () => true,
    'default-skills': () => [],
    'linux-cli-required': () => false,
    'setup-metadata': postgresSetupMetadata,
    'server-directory': () => 'server',
    'find-database-urls': ({ text }: { text: string }) =>
      text.match(/(?:postgresql|postgres):\/\/[^\s'"`]+/g) ?? [],
  }),
  'build-container': operationProvider('postgres', 'build-container', { plan: standardBuildContainerPlan }),
  reference: operationProvider('postgres', 'reference', { deploy: deployPostgresReference }),
  orchestrator: operationProvider('postgres', 'orchestrator', { config: standardOrchestratorConfig }),
}, { version: POSTGRES_ADAPTER_VERSION });

export { postgresAdapter };
