import { POSTGRES_RUNTIME } from './postgres-runtime.js';
import type { GradingCapabilityId } from '../../actions/action-contract.js';
import type { BackendLease } from '../../runtime/backend-lease.js';
import type { TextCommandExecutor } from '../../runtime/command-executor.js';
import { createHttpGradingContext, httpNamedActionRequest } from '../stack-grading-operations.js';
import { captureHostedDiagnostics, activateHosted } from '../hosted-lifecycle.js';
import { postgresConnectionUrl, postgresSetupMetadata,
  standardBuildContainerPlan } from '../stack-agent-operations.js';
import { deployPostgresReference, HOSTED_REFERENCE_LAYOUT } from '../stack-reference-operations.js';
import { standardOrchestratorConfig } from '../stack-orchestrator-operations.js';
import { stopHostedHost } from '../stack-teardown-operations.js';
import { stackLeaseOperations } from '../stack-lease-capabilities.js';
import { getPostgresCheckoutState, getPostgresStock, preparePostgresDatabase, provePostgresUse, resetPostgres,
  setPostgresStock } from './postgres-operations.js';
import { POSTGRES_ADAPTER_VERSION } from './postgres-identity.js';
import { getSavedPostgresCheckoutState } from './postgres-saved-checkout.js';
import { requireLeasedDatabase } from '../backend-reset-guard.js';
import { controlHostedFor, defineStackAdapter } from '../stack-adapter-common.js';

const POSTGRES_GRADING_CAPABILITIES = [
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

const postgresAdapter = defineStackAdapter('postgres', {
  activate: activateHosted,
  control: input => controlHostedFor('postgres', postgresConnectionUrl, input),
}, {
  lease: stackLeaseOperations('postgres'),
  reset: { run: ({ lease, exec }: { lease: BackendLease; exec?: TextCommandExecutor }) =>
    resetPostgres({ lease: requireLeasedDatabase(lease), exec }), requiresReseed: true },
  databaseWrite: { setStock: setPostgresStock },
  databaseRead: { getStock: getPostgresStock, getCheckoutState: getPostgresCheckoutState,
    getSavedCheckoutState: getSavedPostgresCheckoutState },
  diagnostics: { capture: captureHostedDiagnostics },
  database: { prepare: preparePostgresDatabase, proveUse: ({ lease, marker, exec }: { lease: BackendLease; marker: unknown;
    exec?: TextCommandExecutor }) => provePostgresUse({ lease: requireLeasedDatabase(lease), marker, exec }) },
  grading: { context: createHttpGradingContext,
    transport: 'http', capabilities: POSTGRES_GRADING_CAPABILITIES, databaseLease: requireLeasedDatabase },
  namedAction: { request: httpNamedActionRequest },
  teardown: { host: stopHostedHost },
  runPolicy: { resetEnabled: true, retainHostSupported: false,
    supervisorEnvironment: (_input: { spacetimePort: number | null }) => ({}) },
  agent: {
    connectionUrl: postgresConnectionUrl,
    minimalGuidanceSupported: true,
    defaultSkills: [],
    linuxCliRequired: false,
    setupMetadata: postgresSetupMetadata,
    serverDirectory: 'server',
    findDatabaseUrls: ({ text }: { text: string }) =>
      text.match(/(?:postgresql|postgres):\/\/[^\s'"`]+/g) ?? [],
  },
  buildContainer: { plan: standardBuildContainerPlan },
  reference: { layout: HOSTED_REFERENCE_LAYOUT, deploy: deployPostgresReference },
  orchestrator: { config: standardOrchestratorConfig, serverUriVariable: null },
  runtime: POSTGRES_RUNTIME,
}, { version: POSTGRES_ADAPTER_VERSION });

export { postgresAdapter };
