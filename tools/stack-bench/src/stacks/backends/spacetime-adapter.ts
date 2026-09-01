import { createSpacetimeGradingContext,
  spacetimeNamedActionRequest } from '../stack-grading-operations.js';
import { activateSpacetime, controlSpacetime } from '../spacetime-lifecycle.js';
import { noConnectionUrl, spacetimeBuildContainerPlan,
  spacetimeSetupMetadata } from '../stack-agent-operations.js';
import { deploySpacetimeReference } from '../stack-reference-operations.js';
import { spacetimeOrchestratorConfig } from '../stack-orchestrator-operations.js';
import { stopSpacetimeHost } from '../stack-teardown-operations.js';
import { stackLeaseOperations } from '../stack-lease-capabilities.js';
import { prepareSpacetimeDatabase, resetSpacetime, setSpacetimeStock } from './spacetime-operations.js';
import { SPACETIME_ADAPTER_VERSION } from './spacetime-identity.js';
import { defineStackAdapter } from '../stack-adapter-common.js';

export const spacetimeAdapter = defineStackAdapter('spacetime', {
  activate: activateSpacetime,
  control: input => {
    if (input.mode !== 'restart') {
      throw new Error(`unsupported SpacetimeDB control mode ${input.mode}`);
    }
    return controlSpacetime({ lease: input.lease, signal: input.signal });
  },
}, {
  lease: stackLeaseOperations('spacetime'),
  reset: { run: resetSpacetime, requiresReseed: false },
  databaseWrite: { setStock: setSpacetimeStock },
  database: { prepare: prepareSpacetimeDatabase },
  grading: { context: createSpacetimeGradingContext },
  namedAction: { request: spacetimeNamedActionRequest },
  teardown: { host: stopSpacetimeHost },
  runPolicy: { resetEnabled: true, retainHostSupported: true,
    supervisorEnvironment: ({ spacetimePort }: { spacetimePort: number | null }) => {
      if (!spacetimePort) throw new Error('SpacetimeDB supervisor requires a port');
      return { STACK_BENCH_STDB_URI: `http://127.0.0.1:${spacetimePort}` };
    } },
  agent: {
    connectionUrl: (_input: { dbPort: number; database: string; hostUrl(url: string): string }) => noConnectionUrl(),
    minimalGuidanceSupported: false,
    defaultSkills: ['typescript-server', 'typescript-client'],
    linuxCliRequired: true,
    setupMetadata: spacetimeSetupMetadata,
    serverDirectory: 'backend',
    findDatabaseUrls: (_input: { text: string }) => [],
  },
  buildContainer: { plan: spacetimeBuildContainerPlan },
  reference: { deploy: deploySpacetimeReference },
  orchestrator: { config: spacetimeOrchestratorConfig },
}, { version: SPACETIME_ADAPTER_VERSION });
