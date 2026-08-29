import { createHttpGradingContext, httpNamedActionRequest } from '../../actions/stack-action-operations.js';
import { captureHostedDiagnostics, activateHosted } from '../stack-lifecycle-operations.mjs';
import { mongoDbConnectionUrl, mongoDbSetupMetadata,
  standardBuildContainerPlan } from '../stack-agent-operations.mjs';
import { deployMongoDbReference } from '../stack-reference-operations.mjs';
import { standardOrchestratorConfig } from '../stack-orchestrator-operations.mjs';
import { stopHostedHost } from '../stack-teardown-operations.mjs';
import { stackLeaseCapability } from '../stack-lease-capabilities.mjs';
import { prepareMongoDbDatabase, proveMongoDbUse, resetMongoDb,
  setMongoDbStock } from './mongodb-operations.mjs';
import { MONGODB_ADAPTER_VERSION } from './mongodb-identity.js';
import { controlHostedFor, defineStackAdapter, operationProvider,
  runPolicyProvider } from '../stack-adapter-common.js';
import type { StackAdapter } from '../stack-adapter-contract.js';

let mongodbAdapter: StackAdapter;
mongodbAdapter = defineStackAdapter('mongodb', stackLeaseCapability('mongodb'), {
  reset: operationProvider('mongodb', 'reset',
    { run: resetMongoDb, 'requires-reseed': () => true }),
  'database-write': operationProvider('mongodb', 'database-write', { 'set-stock': setMongoDbStock }),
  lifecycle: operationProvider('mongodb', 'lifecycle',
    { activate: activateHosted, control: input => controlHostedFor(mongodbAdapter, input) }),
  diagnostics: operationProvider('mongodb', 'diagnostics', { capture: captureHostedDiagnostics }),
  database: operationProvider('mongodb', 'database',
    { prepare: prepareMongoDbDatabase, 'prove-use': proveMongoDbUse }),
  grading: operationProvider('mongodb', 'grading', { context: createHttpGradingContext }),
  'named-action': operationProvider('mongodb', 'named-action', { request: httpNamedActionRequest }),
  teardown: operationProvider('mongodb', 'teardown', { host: stopHostedHost }),
  'run-policy': runPolicyProvider('mongodb',
    { 'reset-enabled': true, 'sandbox-probe-required': true, 'product-review-enabled': false,
      'product-review-comparisons': [], 'retain-host-supported': false, 'supervisor-env': () => ({}) }),
  agent: operationProvider('mongodb', 'agent', {
    'connection-url': mongoDbConnectionUrl,
    'minimal-guidance-supported': () => true,
    'default-skills': () => [],
    'linux-cli-required': () => false,
    'setup-metadata': mongoDbSetupMetadata,
    'server-directory': () => 'server',
    'find-database-urls': ({ text }: { text: string }) =>
      text.match(/mongodb(?:\+srv)?:\/\/[^\s'"`]+/g) ?? [],
  }),
  'build-container': operationProvider('mongodb', 'build-container', { plan: standardBuildContainerPlan }),
  reference: operationProvider('mongodb', 'reference', { deploy: deployMongoDbReference }),
  orchestrator: operationProvider('mongodb', 'orchestrator', { config: standardOrchestratorConfig }),
}, { version: MONGODB_ADAPTER_VERSION });

export { mongodbAdapter };
