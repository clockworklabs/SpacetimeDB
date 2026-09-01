import { createHttpGradingContext, httpNamedActionRequest } from '../stack-grading-operations.js';
import { captureHostedDiagnostics, activateHosted } from '../hosted-lifecycle.js';
import { postgresConnectionUrl, postgresSetupMetadata,
  standardBuildContainerPlan } from '../stack-agent-operations.js';
import { deployPostgresReference } from '../stack-reference-operations.js';
import { standardOrchestratorConfig } from '../stack-orchestrator-operations.js';
import { stopHostedHost } from '../stack-teardown-operations.js';
import { stackLeaseOperations } from '../stack-lease-capabilities.js';
import { preparePostgresDatabase, provePostgresUse, resetPostgres,
  setPostgresStock } from './postgres-operations.js';
import { POSTGRES_ADAPTER_VERSION } from './postgres-identity.js';
import { controlHostedFor, defineStackAdapter } from '../stack-adapter-common.js';

const postgresAdapter = defineStackAdapter('postgres', {
  activate: activateHosted,
  control: input => controlHostedFor('postgres', postgresConnectionUrl, input),
}, {
  lease: stackLeaseOperations('postgres'),
  reset: { run: resetPostgres, requiresReseed: true },
  databaseWrite: { setStock: setPostgresStock },
  diagnostics: { capture: captureHostedDiagnostics },
  database: { prepare: preparePostgresDatabase, proveUse: provePostgresUse },
  grading: { context: createHttpGradingContext },
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
  reference: { deploy: deployPostgresReference },
  orchestrator: { config: standardOrchestratorConfig },
}, { version: POSTGRES_ADAPTER_VERSION });

export { postgresAdapter };
