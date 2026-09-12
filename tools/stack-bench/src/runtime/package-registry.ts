// The appliance runs a pull-through npm registry cache beside the databases.
// Coding sessions and clean-source starts install through it, so a package
// fetch depends on the public registry only the first time the appliance
// sees that package, never at the moment of a run.
export const PACKAGE_REGISTRY_VARIABLE = 'STACK_BENCH_NPM_REGISTRY';
const LOOPBACK = /^(?:127\.0\.0\.1|localhost)$/;

export function packageRegistry(env: NodeJS.ProcessEnv = process.env): URL | null {
  const value = env[PACKAGE_REGISTRY_VARIABLE];
  if (!value) return null;
  let url: URL;
  try { url = new URL(value); } catch {
    throw new Error(`${PACKAGE_REGISTRY_VARIABLE} is not a URL`);
  }
  if (url.protocol !== 'http:' || !LOOPBACK.test(url.hostname) || !url.port
    || url.pathname !== '/' || url.search || url.hash || url.username || url.password) {
    throw new Error(`${PACKAGE_REGISTRY_VARIABLE} must be http://127.0.0.1:<port>/`);
  }
  return url;
}

// npm reads its registry from the environment, so one variable at container
// creation covers the agent's own installs and every later `docker exec`,
// including the clean-source start.
export function packageRegistryEnvironment(registry: URL | null,
  networkMode: string | null | undefined, network?: BackendLeaseNetwork): Record<string, string> {
  if (network) {
    const service = network.services.find(value => value.port === 4873);
    if (!network.cacheContainerId || !service) throw new Error('attempt has no authenticated cache connection');
    // Lockfiles must survive fresh attempt IPs. Docker resolves this name on each owned network.
    return { NPM_CONFIG_REGISTRY: `http://stack-bench-npm-cache:${service.port}/` };
  }
  if (!registry) return {};
  const host = networkMode === 'host' ? '127.0.0.1' : 'host.docker.internal';
  return { NPM_CONFIG_REGISTRY: `http://${host}:${registry.port}/` };
}
import type { BackendLeaseNetwork } from './backend-lease.js';
