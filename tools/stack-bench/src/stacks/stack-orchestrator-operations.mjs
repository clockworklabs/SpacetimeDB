import { join } from 'node:path';

export function spacetimeOrchestratorConfig({ root, env, helpers }) {
  const serverUri = env.STACK_BENCH_STDB_URI ?? 'http://127.0.0.1:3210';
  const target = new URL(serverUri);
  if (!['127.0.0.1', 'localhost', '[::1]'].includes(target.hostname) || !target.port) {
    throw new Error(`STACK_BENCH_STDB_URI must be an explicit loopback port, got ${serverUri}`);
  }
  const localCli = join(root, '..', '..', 'target', 'release', 'spacetimedb-cli.exe');
  const cli = env.SPACETIME_BIN ?? (helpers.exists(localCli) ? localCli : 'spacetime');
  return {
    environment: { STACK_BENCH_STDB_URI: serverUri, SPACETIME_BIN: cli },
    lease: { serverUri }, lifecycle: { cli },
    windowsEnvironmentBridge: ['STACK_BENCH_STDB_URI', 'SPACETIME_BIN/p'],
  };
}

export function standardOrchestratorConfig() {
  return { environment: {}, lease: { serverUri: null }, lifecycle: {}, windowsEnvironmentBridge: [] };
}
