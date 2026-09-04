import type { GradingCapabilityId } from '../../actions/action-contract.js';
import { createHttpGradingContext, httpNamedActionRequest } from '../stack-grading-operations.js';
import { captureHostedDiagnostics, activateHosted } from '../hosted-lifecycle.js';
import { mongoDbConnectionUrl, mongoDbSetupMetadata,
  standardBuildContainerPlan } from '../stack-agent-operations.js';
import { deployMongoDbReference } from '../stack-reference-operations.js';
import { standardOrchestratorConfig } from '../stack-orchestrator-operations.js';
import { stopHostedHost } from '../stack-teardown-operations.js';
import { stackLeaseOperations } from '../stack-lease-capabilities.js';
import { prepareMongoDbDatabase, proveMongoDbUse, resetMongoDb,
  setMongoDbStock } from './mongodb-operations.js';
import { MONGODB_ADAPTER_VERSION } from './mongodb-identity.js';
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
  'named-actions',
  'subprocess',
  'transport-observation',
] as const satisfies readonly GradingCapabilityId[];

const mongodbAdapter = defineStackAdapter('mongodb', {
  activate: activateHosted,
  control: input => controlHostedFor('mongodb', mongoDbConnectionUrl, input),
}, {
  lease: stackLeaseOperations('mongodb'),
  reset: { run: resetMongoDb, requiresReseed: true },
  databaseWrite: { setStock: setMongoDbStock },
  diagnostics: { capture: captureHostedDiagnostics },
  database: { prepare: prepareMongoDbDatabase, proveUse: proveMongoDbUse },
  grading: { context: createHttpGradingContext,
    transport: 'http', capabilities: MONGODB_GRADING_CAPABILITIES },
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
  reference: { deploy: deployMongoDbReference },
  orchestrator: { config: standardOrchestratorConfig },
}, { version: MONGODB_ADAPTER_VERSION });

export { mongodbAdapter };
