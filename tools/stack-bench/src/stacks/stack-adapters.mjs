import { createStackAdapterRegistry, executeStackCapability,
  STACK_ADAPTER_SCHEMA_VERSION, STACK_CAPABILITY_SCHEMA_VERSION } from './stack-adapter-contract.mjs';
import { prepareMongoDbDatabase, preparePostgresDatabase, prepareResourceFreeDatabase,
  prepareSpacetimeDatabase, resetMongoDb, resetPostgres, resetSpacetime,
  setMongoDbStock, setPostgresStock, setSpacetimeStock } from './stack-backend-operations.mjs';
import { activateHosted, activateSpacetime, captureHostedDiagnostics,
  controlHosted, controlSpacetime } from './stack-lifecycle-operations.mjs';
import { createHttpGradingContext, createSpacetimeGradingContext,
  httpNamedActionRequest, spacetimeNamedActionRequest } from '../actions/stack-action-operations.mjs';
import { stopHostedHost, stopSpacetimeHost } from './stack-teardown-operations.mjs';
import { emptySetupMetadata, mongoDbConnectionUrl, mongoDbSetupMetadata,
  noConnectionUrl, postgresConnectionUrl, postgresSetupMetadata,
  spacetimeBuildContainerPlan, spacetimeSetupMetadata,
  standardBuildContainerPlan } from './stack-agent-operations.mjs';
import { deployMongoDbReference, deployPostgresReference,
  deploySpacetimeReference } from './stack-reference-operations.mjs';
import { spacetimeOrchestratorConfig,
  standardOrchestratorConfig } from './stack-orchestrator-operations.mjs';
import { stackLeaseCapability } from './stack-lease-capabilities.mjs';

const PORT_BASES = Object.freeze({
  spacetime: Object.freeze({ vite: 6173 }),
  postgres: Object.freeze({ vite: 6273, express: 6001, db: 6532 }),
  mongodb: Object.freeze({ vite: 6373, express: 6101, db: 6537 }),
  stub: Object.freeze({ vite: 7000 }),
});

function capability(id, operations, execute) {
  return { schemaVersion: STACK_CAPABILITY_SCHEMA_VERSION, id, version: '1.0.0', operations, execute };
}

function portsProvider(adapterId) {
  const allocations = PORT_BASES[adapterId];
  return capability(`${adapterId}.ports`, ['allocations', 'for-run'], (operation, input) => {
    if (operation === 'allocations') return { ...allocations };
    if (!Number.isInteger(input.trackOffset) || input.trackOffset < 0
      || !Number.isInteger(input.runIndex) || input.runIndex < 0) {
      throw new Error(`${adapterId} ports require non-negative integer trackOffset and runIndex`);
    }
    const offset = Number(input.trackOffset) + Number(input.runIndex);
    return {
      vite: allocations.vite + offset,
      express: allocations.express ? allocations.express + offset : null,
      dbPort: allocations.db ?? null,
    };
  });
}

function operationProvider(adapterId, name, operations) {
  return capability(`${adapterId}.${name}`, Object.keys(operations), (operation, input) =>
    operations[operation](input));
}

function runPolicyProvider(adapterId, values) {
  return operationProvider(adapterId, 'run-policy', Object.fromEntries(
    Object.entries(values).map(([name, value]) => [name, typeof value === 'function' ? value : () => value])));
}

function adapter(id, lease, capabilities = {}, { version = '1.0.0' } = {}) {
  return {
    schemaVersion: STACK_ADAPTER_SCHEMA_VERSION,
    id, version,
    capabilities: { ports: portsProvider(id), lease, ...capabilities },
  };
}

export const STACK_ADAPTER_REGISTRY = createStackAdapterRegistry([
  adapter('spacetime', stackLeaseCapability('spacetime'), {
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
        'product-review-comparisons': ['postgres', 'mongodb'],
        'retain-host-supported': true,
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
  }),
  adapter('postgres', stackLeaseCapability('postgres'), {
    reset: operationProvider('postgres', 'reset',
      { run: resetPostgres, 'requires-reseed': () => true }),
    'database-write': operationProvider('postgres', 'database-write', { 'set-stock': setPostgresStock }),
    lifecycle: operationProvider('postgres', 'lifecycle',
      { activate: activateHosted, control: controlHosted }),
    diagnostics: operationProvider('postgres', 'diagnostics', { capture: captureHostedDiagnostics }),
    database: operationProvider('postgres', 'database', { prepare: preparePostgresDatabase }),
    grading: operationProvider('postgres', 'grading', { context: createHttpGradingContext }),
    'named-action': operationProvider('postgres', 'named-action', { request: httpNamedActionRequest }),
    teardown: operationProvider('postgres', 'teardown', { host: stopHostedHost }),
    'run-policy': runPolicyProvider('postgres',
      { 'reset-enabled': true, 'sandbox-probe-required': true, 'product-review-enabled': false,
        'product-review-comparisons': [],
        'retain-host-supported': false,
        'supervisor-env': () => ({}) }),
    agent: operationProvider('postgres', 'agent', {
      'connection-url': postgresConnectionUrl,
      'minimal-guidance-supported': () => true,
      'default-skills': () => [],
      'linux-cli-required': () => false,
      'setup-metadata': postgresSetupMetadata,
      'server-directory': () => 'server',
      'find-database-urls': ({ text }) => text.match(/(?:postgresql|postgres):\/\/[^\s'"`]+/g) ?? [],
    }),
    'build-container': operationProvider('postgres', 'build-container', { plan: standardBuildContainerPlan }),
    reference: operationProvider('postgres', 'reference', { deploy: deployPostgresReference }),
    orchestrator: operationProvider('postgres', 'orchestrator', { config: standardOrchestratorConfig }),
  }, { version: '1.1.0' }),
  adapter('mongodb', stackLeaseCapability('mongodb'), {
    reset: operationProvider('mongodb', 'reset',
      { run: resetMongoDb, 'requires-reseed': () => true }),
    'database-write': operationProvider('mongodb', 'database-write', { 'set-stock': setMongoDbStock }),
    lifecycle: operationProvider('mongodb', 'lifecycle',
      { activate: activateHosted, control: controlHosted }),
    diagnostics: operationProvider('mongodb', 'diagnostics', { capture: captureHostedDiagnostics }),
    database: operationProvider('mongodb', 'database', { prepare: prepareMongoDbDatabase }),
    grading: operationProvider('mongodb', 'grading', { context: createHttpGradingContext }),
    'named-action': operationProvider('mongodb', 'named-action', { request: httpNamedActionRequest }),
    teardown: operationProvider('mongodb', 'teardown', { host: stopHostedHost }),
    'run-policy': runPolicyProvider('mongodb',
      { 'reset-enabled': true, 'sandbox-probe-required': true, 'product-review-enabled': false,
        'product-review-comparisons': [],
        'retain-host-supported': false,
        'supervisor-env': () => ({}) }),
    agent: operationProvider('mongodb', 'agent', {
      'connection-url': mongoDbConnectionUrl,
      'minimal-guidance-supported': () => true,
      'default-skills': () => [],
      'linux-cli-required': () => false,
      'setup-metadata': mongoDbSetupMetadata,
      'server-directory': () => 'server',
      'find-database-urls': ({ text }) => text.match(/mongodb(?:\+srv)?:\/\/[^\s'"`]+/g) ?? [],
    }),
    'build-container': operationProvider('mongodb', 'build-container', { plan: standardBuildContainerPlan }),
    reference: operationProvider('mongodb', 'reference', { deploy: deployMongoDbReference }),
    orchestrator: operationProvider('mongodb', 'orchestrator', { config: standardOrchestratorConfig }),
  }, { version: '1.1.0' }),
  adapter('stub', stackLeaseCapability('stub'), {
    reset: operationProvider('stub', 'reset', { 'requires-reseed': () => false }),
    lifecycle: operationProvider('stub', 'lifecycle', { activate: activateHosted }),
    database: operationProvider('stub', 'database', { prepare: prepareResourceFreeDatabase }),
    grading: operationProvider('stub', 'grading', { context: createHttpGradingContext }),
    'named-action': operationProvider('stub', 'named-action', { request: httpNamedActionRequest }),
    teardown: operationProvider('stub', 'teardown', { host: stopHostedHost }),
    'run-policy': runPolicyProvider('stub',
      { 'reset-enabled': false, 'sandbox-probe-required': false, 'product-review-enabled': false,
        'product-review-comparisons': [],
        'retain-host-supported': false,
        'supervisor-env': () => ({}) }),
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
  }),
]);

export function stackPortAllocations() {
  return Object.fromEntries(STACK_ADAPTER_REGISTRY.ids.map(id => [id,
    executeStackCapability(STACK_ADAPTER_REGISTRY.get(id), 'ports', 'allocations')]));
}
