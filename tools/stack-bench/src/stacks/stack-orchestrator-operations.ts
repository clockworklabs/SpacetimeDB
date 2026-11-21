import { join } from 'node:path';

import { DEFAULT_SPACETIME_SERVER_URI, loopbackHttpUri } from '../runtime/backend-lease.js';
import { DEFAULT_CONVEX_SERVER_URI } from './backends/convex-identity.js';

interface OrchestratorConfig {
  environment: Record<string, string>;
  lease: { serverUri: string | null };
  lifecycle: { cli?: string };
  windowsEnvironmentBridge: string[];
}

export function spacetimeOrchestratorConfig({ root, env, helpers }: {
  root: string;
  env: NodeJS.ProcessEnv;
  helpers: { exists: (path: string) => boolean };
}): OrchestratorConfig {
  const serverUri = env.STACK_BENCH_STDB_URI ?? DEFAULT_SPACETIME_SERVER_URI;
  loopbackHttpUri(serverUri);
  const localCli = join(root, '..', '..', 'target', 'release', 'spacetimedb-cli.exe');
  const cli = env.SPACETIME_BIN ?? (helpers.exists(localCli) ? localCli : 'spacetime');
  return {
    environment: { STACK_BENCH_STDB_URI: serverUri, SPACETIME_BIN: cli },
    lease: { serverUri }, lifecycle: { cli },
    windowsEnvironmentBridge: ['STACK_BENCH_STDB_URI', 'SPACETIME_BIN/p'],
  };
}

export function standardOrchestratorConfig(): OrchestratorConfig {
  return { environment: {}, lease: { serverUri: null }, lifecycle: {}, windowsEnvironmentBridge: [] };
}

export function convexOrchestratorConfig({ env }: { env: NodeJS.ProcessEnv }): OrchestratorConfig {
  const serverUri = env.STACK_BENCH_CONVEX_URI ?? DEFAULT_CONVEX_SERVER_URI;
  loopbackHttpUri(serverUri);
  return { environment: { STACK_BENCH_CONVEX_URI: serverUri }, lease: { serverUri },
    lifecycle: {}, windowsEnvironmentBridge: ['STACK_BENCH_CONVEX_URI'] };
}
