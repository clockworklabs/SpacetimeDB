import { CONVEX_RUNTIME } from './convex-runtime.js';
import type { GradingCapabilityId } from '../../actions/action-contract.js';
import { createHttpGradingContext } from '../stack-grading-operations.js';
import { captureHostedDiagnostics } from '../hosted-lifecycle.js';
import { noConnectionUrl, convexSetupMetadata, standardBuildContainerPlan } from '../stack-agent-operations.js';
import { deployConvexReference, CONVEX_REFERENCE_LAYOUT } from '../stack-reference-operations.js';
import { convexOrchestratorConfig } from '../stack-orchestrator-operations.js';
import { stopHostedHost } from '../stack-teardown-operations.js';
import { stackLeaseOperations } from '../stack-lease-capabilities.js';
import { activateConvex, controlConvexApplication, resetConvex } from './convex-lifecycle.js';
import { convexApplicationEnvironment, convexNamedActionRequest, probeConvexNamedAction, proveConvexUse,
  setConvexStock, getConvexStock, getConvexCheckoutState } from './convex-operations.js';
import { CONVEX_ADAPTER_VERSION } from './convex-identity.js';
import { defineStackAdapter } from '../stack-adapter-common.js';

const capabilities = [
  'actors', 'application-files', 'application-lifecycle', 'backend-lifecycle',
  'browser-interaction', 'browser-observation', 'clock', 'concurrency',
  'database-write', 'database-read', 'named-actions', 'process-crash', 'response-loss', 'subprocess', 'transport-observation',
] as const satisfies readonly GradingCapabilityId[];

export const convexAdapter = defineStackAdapter('convex', {
  activate: activateConvex, control: controlConvexApplication, applicationEnvironment: convexApplicationEnvironment,
}, {
  lease: stackLeaseOperations('convex'),
  reset: { run: resetConvex, requiresReseed: true },
  databaseWrite: { setStock: setConvexStock },
  databaseRead: { getStock: getConvexStock, getCheckoutState: getConvexCheckoutState },
  diagnostics: { capture: captureHostedDiagnostics },
  database: { proveUse: proveConvexUse },
  grading: { context: createHttpGradingContext, transport: 'convex', capabilities },
  namedAction: { request: convexNamedActionRequest, browserAccounts: ['signUp', 'signIn'], probe: probeConvexNamedAction },
  teardown: { host: stopHostedHost },
  runPolicy: { resetEnabled: true, retainHostSupported: false,
    supervisorEnvironment: (_input: { spacetimePort: number | null }) => ({}) },
  agent: {
    connectionUrl: (_input: { dbPort: number; database: string; hostUrl(url: string): string }) => noConnectionUrl(),
    minimalGuidanceSupported: true, defaultSkills: [], linuxCliRequired: false,
    setupMetadata: convexSetupMetadata, serverDirectory: 'convex',
    findDatabaseUrls: (_input: { text: string }) => [],
  },
  buildContainer: { plan: standardBuildContainerPlan },
  reference: { layout: CONVEX_REFERENCE_LAYOUT, deploy: deployConvexReference },
  orchestrator: { config: convexOrchestratorConfig, serverUriVariable: 'STACK_BENCH_CONVEX_URI' },
  runtime: CONVEX_RUNTIME,
}, { version: CONVEX_ADAPTER_VERSION });
