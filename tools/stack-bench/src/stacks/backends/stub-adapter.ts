import { GRADING_CAPABILITY_IDS } from '../../actions/action-contract.js';
import { createHttpGradingContext, httpNamedActionRequest } from '../stack-grading-operations.js';
import { activateHosted } from '../hosted-lifecycle.js';
import { emptySetupMetadata, noConnectionUrl,
  standardBuildContainerPlan } from '../stack-agent-operations.js';
import { standardOrchestratorConfig } from '../stack-orchestrator-operations.js';
import { stopHostedHost } from '../stack-teardown-operations.js';
import { stackLeaseOperations } from '../stack-lease-capabilities.js';
import { STUB_ADAPTER_VERSION } from './stub-identity.js';
import { defineStackAdapter } from '../stack-adapter-common.js';

export const stubAdapter = defineStackAdapter('stub', {
  activate: activateHosted,
}, {
  lease: stackLeaseOperations('stub'),
  reset: { requiresReseed: false },
  database: { prepare: ({ name }: { name: string }) => name },
  // The stub has no backend to restart and no database to write to.
  grading: { context: createHttpGradingContext, transport: 'http',
    capabilities: GRADING_CAPABILITY_IDS.filter(capability =>
      capability !== 'backend-lifecycle' && capability !== 'database-write') },
  namedAction: { request: httpNamedActionRequest },
  teardown: { host: stopHostedHost },
  runPolicy: { resetEnabled: false, retainHostSupported: false,
    supervisorEnvironment: (_input: { spacetimePort: number | null }) => ({}) },
  admission: {
    requirements: { docker: false, services: false, ports: false,
      credentials: false, providerAccess: false },
  },
  agent: {
    connectionUrl: (_input: { dbPort: number; database: string; hostUrl(url: string): string }) => noConnectionUrl(),
    minimalGuidanceSupported: true,
    defaultSkills: [],
    linuxCliRequired: false,
    setupMetadata: emptySetupMetadata,
    serverDirectory: 'server',
    findDatabaseUrls: (_input: { text: string }) => [],
  },
  buildContainer: { plan: () => standardBuildContainerPlan() },
  orchestrator: { config: standardOrchestratorConfig },
}, { version: STUB_ADAPTER_VERSION });
