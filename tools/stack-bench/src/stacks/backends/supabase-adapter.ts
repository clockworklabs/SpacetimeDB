import type { GradingCapabilityId } from '../../actions/action-contract.js';
import type { BackendLease } from '../../runtime/backend-lease.js';
import { requireLeasedDatabase, type LeasedDatabase } from '../backend-reset-guard.js';
import { createHttpGradingContext } from '../stack-grading-operations.js';
import { captureHostedDiagnostics } from '../hosted-lifecycle.js';
import { noConnectionUrl, standardBuildContainerPlan } from '../stack-agent-operations.js';
import { stopHostedHost } from '../stack-teardown-operations.js';
import { stackLeaseOperations } from '../stack-lease-capabilities.js';
import { activateSupabase, controlSupabaseApplication, resetSupabase, supabaseApplicationEnvironment,
  supabaseOrchestratorConfig, SUPABASE_RUNTIME } from './supabase-lifecycle.js';
import { getSupabaseCheckoutState, getSupabaseStock, proveSupabaseUse, setSupabaseStock,
  supabaseAuthRequestPatch, supabaseNamedActionRequest, supabaseWriteEndpoints } from './supabase-operations.js';
import { supabaseSetupMetadata } from './supabase-agent.js';
import { deploySupabaseReference, SUPABASE_REFERENCE_LAYOUT } from './supabase-reference.js';
import { SUPABASE_ADAPTER_VERSION } from './supabase-identity.js';
import { defineStackAdapter } from '../stack-adapter-common.js';

const capabilities = [
  'actors', 'application-files', 'application-lifecycle', 'backend-lifecycle',
  'browser-interaction', 'browser-observation', 'clock', 'concurrency',
  'database-write', 'database-read', 'named-actions', 'process-crash', 'response-loss', 'subprocess', 'transport-observation',
] as const satisfies readonly GradingCapabilityId[];

export const supabaseAdapter = defineStackAdapter('supabase', {
  activate: activateSupabase, control: controlSupabaseApplication, applicationEnvironment: supabaseApplicationEnvironment,
}, {
  lease: stackLeaseOperations('supabase'),
  reset: { run: resetSupabase, requiresReseed: true },
  databaseWrite: { setStock: setSupabaseStock },
  databaseRead: { getStock: getSupabaseStock, getCheckoutState: getSupabaseCheckoutState },
  diagnostics: { capture: captureHostedDiagnostics },
  database: { proveUse: proveSupabaseUse },
  // Writes are ordinary HTTP to the gateway; named operations are database functions. Privileged
  // observers act on the whole platform lease, which also names the database container.
  grading: { context: createHttpGradingContext, transport: 'http', capabilities, namedActionBinding: 'reducer',
    databaseLease: (lease: BackendLease) => { requireLeasedDatabase(lease); return lease as BackendLease & LeasedDatabase; },
    writeEndpoints: supabaseWriteEndpoints, authRequestPatch: supabaseAuthRequestPatch },
  namedAction: { request: supabaseNamedActionRequest, browserAccounts: ['signUp', 'signIn'] },
  teardown: { host: stopHostedHost },
  runPolicy: { resetEnabled: true, retainHostSupported: false,
    supervisorEnvironment: (_input: { spacetimePort: number | null }) => ({}) },
  agent: {
    connectionUrl: (_input: { dbPort: number; database: string; hostUrl(url: string): string }) => noConnectionUrl(),
    minimalGuidanceSupported: true, defaultSkills: [], linuxCliRequired: false,
    setupMetadata: supabaseSetupMetadata, serverDirectory: 'supabase',
    findDatabaseUrls: (_input: { text: string }) => [],
  },
  buildContainer: { plan: standardBuildContainerPlan },
  reference: { layout: SUPABASE_REFERENCE_LAYOUT, deploy: deploySupabaseReference },
  orchestrator: { config: supabaseOrchestratorConfig, serverUriVariable: 'STACK_BENCH_SUPABASE_URI' },
  runtime: SUPABASE_RUNTIME,
}, { version: SUPABASE_ADAPTER_VERSION });
