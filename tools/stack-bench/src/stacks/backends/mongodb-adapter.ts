import { MONGODB_RUNTIME } from './mongodb-runtime.js';
import type { GradingCapabilityId } from '../../actions/action-contract.js';
import type { BackendLease } from '../../runtime/backend-lease.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';
import { createHttpGradingContext, httpNamedActionRequest } from '../stack-grading-operations.js';
import { captureHostedDiagnostics, activateHosted } from '../hosted-lifecycle.js';
import { mongoDbConnectionUrl, mongoDbSetupMetadata,
  standardBuildContainerPlan } from '../stack-agent-operations.js';
import { deployMongoDbReference, HOSTED_REFERENCE_LAYOUT } from '../stack-reference-operations.js';
import { standardOrchestratorConfig } from '../stack-orchestrator-operations.js';
import { stopHostedHost } from '../stack-teardown-operations.js';
import { stackLeaseOperations } from '../stack-lease-capabilities.js';
import { prepareMongoDbDatabase, proveMongoDbUse, resetMongoDb,
  setMongoDbStock, getMongoDbStock, getMongoDbCheckoutState } from './mongodb-operations.js';
import { MONGODB_ADAPTER_VERSION } from './mongodb-identity.js';
import { getSavedMongoDbCheckoutState } from './mongodb-saved-checkout.js';
import { requireLeasedDatabase } from '../backend-reset-guard.js';
import { controlHostedFor, defineStackAdapter } from '../stack-adapter-common.js';

const MONGODB_GRADING_CAPABILITIES = [
  'actors',
  'application-files',
  'application-lifecycle',
  'backend-lifecycle',
  'browser-interaction',
  'browser-observation',
  'clock',
  'concurrency',
  'database-write',
  'database-read',
  'named-actions',
  'process-crash',
  'response-loss',
  'subprocess',
  'transport-observation',
] as const satisfies readonly GradingCapabilityId[];

const mongodbAdapter = defineStackAdapter('mongodb', {
  activate: activateHosted,
  control: input => controlHostedFor('mongodb', mongoDbConnectionUrl, input),
}, {
  lease: stackLeaseOperations('mongodb'),
  reset: { run: ({ lease, exec }: { lease: BackendLease; exec?: TextCommandExecutor }) =>
    resetMongoDb({ lease: requireLeasedDatabase(lease), exec }), requiresReseed: true },
  databaseWrite: { setStock: setMongoDbStock },
  databaseRead: { getStock: getMongoDbStock, getCheckoutState: getMongoDbCheckoutState,
    getSavedCheckoutState: getSavedMongoDbCheckoutState },
  diagnostics: { capture: captureHostedDiagnostics },
  database: { prepare: prepareMongoDbDatabase, proveUse: ({ lease, marker, exec }: { lease: BackendLease; marker: unknown;
    exec?: TextCommandExecutor }) => proveMongoDbUse({ lease: requireLeasedDatabase(lease), marker, exec }) },
  grading: { context: createHttpGradingContext,
    transport: 'http', capabilities: MONGODB_GRADING_CAPABILITIES, databaseLease: requireLeasedDatabase },
  namedAction: { request: httpNamedActionRequest },
  teardown: { host: stopHostedHost },
  runPolicy: { resetEnabled: true, retainHostSupported: false,
    supervisorEnvironment: (_input: { spacetimePort: number | null }) => ({}) },
  agent: {
    connectionUrl: mongoDbConnectionUrl,
    minimalGuidanceSupported: true,
    defaultSkills: [],
    linuxCliRequired: false,
    setupMetadata: mongoDbSetupMetadata,
    serverDirectory: 'server',
    findDatabaseUrls: ({ text }: { text: string }) =>
      text.match(/mongodb(?:\+srv)?:\/\/[^\s'"`]+/g) ?? [],
  },
  buildContainer: { plan: standardBuildContainerPlan },
  reference: { layout: HOSTED_REFERENCE_LAYOUT, deploy: deployMongoDbReference },
  orchestrator: { config: standardOrchestratorConfig, serverUriVariable: null },
  runtime: MONGODB_RUNTIME,
}, { version: MONGODB_ADAPTER_VERSION });

export { mongodbAdapter };
