import { isIPv4 } from 'node:net';
import { execFileSync } from 'node:child_process';
import { createHash, randomBytes } from 'node:crypto';
import { networkInterfaces } from 'node:os';
import { readBackendLease, updateBackendLease } from './backend-lease.js';
import type { BackendCreationKind, BackendLease, BackendLeaseContainer } from './backend-lease.js';
import { SIDECAR_CONTAINER_RESOURCE_LIMITS } from '../composition/product-config.js';
import { ATTEMPT_CREATION_LABEL } from './container-identity.js';
export { ATTEMPT_CREATION_LABEL } from './container-identity.js';

export function attemptDocker(args: string[], input?: string): string {
  return execFileSync('docker', args, { encoding: 'utf8', stdio: 'pipe', input,
    timeout: 30_000, windowsHide: true }).trim();
}

export function attemptControllerImage(): string {
  const image = process.env.STACK_BENCH_CONTROLLER_IMAGE_ID;
  if (!image || !/^sha256:[a-f0-9]{64}$/.test(image)) {
    throw new Error('an immutable STACK_BENCH_CONTROLLER_IMAGE_ID is required');
  }
  return image;
}

export function recordAttemptCreation(leasePath: string, lease: BackendLease, kind: BackendCreationKind) {
  const intent = { name: `sb-${createHash('sha256').update(lease.runId).digest('hex').slice(0, 16)}-${kind}`,
    creationToken: randomBytes(16).toString('hex') };
  updateBackendLease(leasePath, { token: lease.ownershipToken }, next => {
    if (next.resources.creationIntents?.[kind]) throw new Error(`${kind} already has creation authority; recover the attempt first`);
    (next.resources.creationIntents ??= {})[kind] = intent;
    return next;
  });
  return intent;
}

export function createAttemptNetwork(leasePath: string, lease: BackendLease): BackendLease {
  const intent = recordAttemptCreation(leasePath, lease, 'network');
  const id = attemptDocker(['network', 'create', '--driver', 'bridge',
    '--label', `${ATTEMPT_CREATION_LABEL}=${intent.creationToken}`, intent.name]);
  const inspected = JSON.parse(attemptDocker(['network', 'inspect', id]))[0];
  const hostAddresses = [...new Set([...Object.values(networkInterfaces()).flatMap(entries =>
    (entries ?? []).filter(value => value.family === 'IPv4').map(value => value.address)),
    ...inspected.IPAM.Config.map((value: { Gateway?: string }) => value.Gateway).filter(Boolean)])] as string[];
  return updateBackendLease(leasePath, { token: lease.ownershipToken }, next => {
    next.resources.network = { name: intent.name, id, namespaceContainerId: null,
      hostAddresses, services: [], firewallSha256: null, firewallInstalledAt: null };
    return next;
  });
}

export function createAttemptContainer(leasePath: string, lease: BackendLease, kind: 'backend' | 'browser',
  image: string, networkMode: string, args: string[]): BackendLeaseContainer {
  const intent = recordAttemptCreation(leasePath, lease, kind);
  const id = attemptDocker(['create', '--name', intent.name,
    '--label', `${ATTEMPT_CREATION_LABEL}=${intent.creationToken}`,
    '--network', networkMode, '--init', '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true',
    '--cpus', String(SIDECAR_CONTAINER_RESOURCE_LIMITS.cpuCount),
    '--memory', String(SIDECAR_CONTAINER_RESOURCE_LIMITS.memoryBytes),
    '--memory-swap', String(SIDECAR_CONTAINER_RESOURCE_LIMITS.memoryBytes),
    '--pids-limit', String(SIDECAR_CONTAINER_RESOURCE_LIMITS.pids),
    ...args, '--entrypoint', '/bin/sh', image, '-c', 'exec sleep infinity']);
  const container: BackendLeaseContainer = { name: intent.name, id, image, owned: true, networkMode };
  updateBackendLease(leasePath, { token: lease.ownershipToken }, next => {
    if (kind === 'backend') {
      next.resources.container = container;
      next.resources.network!.namespaceContainerId = id;
    } else next.resources.browserContainer = container;
    return next;
  });
  attemptDocker(['start', id]);
  if (kind === 'backend') {
    // Every container that joins this namespace shares the anchor's address on the
    // attempt network; a coding agent probing its own application there is not
    // reaching another run.
    const joined = JSON.parse(attemptDocker(['inspect', '--format', '{{json .NetworkSettings.Networks}}', id]));
    const ownAddresses = Object.values(joined).flatMap((value: unknown) =>
      typeof value === 'object' && value !== null && 'IPAddress' in value
        && typeof value.IPAddress === 'string' && isIPv4(value.IPAddress) ? [value.IPAddress] : []);
    updateBackendLease(leasePath, { token: lease.ownershipToken }, next => {
      next.resources.network!.ownAddresses = ownAddresses;
      return next;
    });
    const startedAt = attemptDocker(['inspect', '--format', '{{.State.StartedAt}}', id]);
    updateBackendLease(leasePath, { token: lease.ownershipToken }, next => {
      next.resources.network!.namespaceStartedAt = startedAt;
      return next;
    });
  }
  return container;
}

export function installAttemptFirewall(leasePath: string, lease: BackendLease,
  docker: typeof attemptDocker = attemptDocker): BackendLease {
  const network = lease.resources.network;
  if (!network?.namespaceContainerId) throw new Error('attempt has no backend namespace');
  const cacheId = docker(['inspect', '--format', '{{.Id}}', 'stack-bench-npm-cache']);
  updateBackendLease(leasePath, { token: lease.ownershipToken }, next => {
    next.resources.network!.cacheContainerId = cacheId;
    return next;
  });
  // Temporary attempt networks must not replace the shared cache's default route.
  docker(['network', 'connect', '--gw-priority', '-1', network.id, cacheId]);
  const connections = JSON.parse(docker(['inspect', '--format', '{{json .NetworkSettings.Networks}}', cacheId]));
  const connection = Object.values(connections).find((value: unknown) =>
    typeof value === 'object' && value !== null && 'NetworkID' in value && value.NetworkID === network.id);
  if (!connection || typeof connection !== 'object' || !('IPAddress' in connection)
    || typeof connection.IPAddress !== 'string') throw new Error('cache did not join the owned network');
  const services = [{ address: connection.IPAddress, port: 4873 }];
  const rules = attemptNetworkRules({ services, hostAddresses: network.hostAddresses });
  const intent = recordAttemptCreation(leasePath, lease, 'firewall');
  docker(['run', '--rm', '-i', '--name', intent.name,
    '--label', `${ATTEMPT_CREATION_LABEL}=${intent.creationToken}`,
    '--network', `container:${network.namespaceContainerId}`, '--cap-drop', 'ALL', '--cap-add', 'NET_ADMIN',
    '--security-opt', 'no-new-privileges:true', '--read-only', '--entrypoint', 'nft',
    attemptControllerImage(), '-f', '-'], rules);
  return updateBackendLease(leasePath, { token: lease.ownershipToken }, next => {
    next.resources.network!.services = services;
    next.resources.network!.firewallSha256 = createHash('sha256').update(rules).digest('hex');
    next.resources.network!.firewallInstalledAt = new Date().toISOString();
    return next;
  });
}

export function requireAttemptNetwork(lease: BackendLease): string {
  const network = lease.resources.network;
  if (!network?.namespaceContainerId || !network.firewallSha256 || !network.firewallInstalledAt) {
    throw new Error('attempt network is not isolated; activation or authenticated recovery is required');
  }
  const state = JSON.parse(attemptDocker(['inspect', '--format', '{{json .State}}', network.namespaceContainerId]));
  if (!state.Running || state.StartedAt !== network.namespaceStartedAt) {
    throw new Error('attempt namespace anchor stopped or restarted; recover the entire attempt');
  }
  return `container:${network.namespaceContainerId}`;
}

export function createAttemptBrowser(leasePath: string, lease: BackendLease): void {
  const current = readBackendLease(leasePath, { token: lease.ownershipToken });
  createAttemptContainer(leasePath, current, 'browser', attemptControllerImage(), requireAttemptNetwork(current),
    ['--read-only', '--tmpfs', '/tmp:rw,nosuid,size=512m',
      '--shm-size', String(SIDECAR_CONTAINER_RESOURCE_LIMITS.memoryBytes),
      '-e', 'HOME=/tmp', '-e', 'XDG_CONFIG_HOME=/tmp/.config', '-e', 'XDG_CACHE_HOME=/tmp/.cache']);
}

export const DOCKER_HOST_ALIAS = 'host.docker.internal';

// Applied only to the attempt's network namespace, before any untrusted process.
// Public HTTPS/HTTP is intentional: it is internet access, not an offline policy.
export function attemptNetworkRules({ services, hostAddresses }: {
  services: readonly { address: string; port: number }[];
  hostAddresses: readonly string[];
}): string {
  const address = (value: string): string => {
    if (!isIPv4(value)) throw new Error('attempt firewall requires literal IPv4 addresses');
    return value;
  };
  const hosts = hostAddresses.map(address);
  if (hosts.length === 0) throw new Error('attempt firewall requires host addresses');
  const allows = services.map(service => {
    address(service.address);
    if (hosts.includes(service.address)) throw new Error('attempt service cannot be a host address');
    if (!Number.isInteger(service.port) || service.port < 1 || service.port > 65535) {
      throw new Error('attempt service port is invalid');
    }
    return `    ip daddr ${service.address} tcp dport ${service.port} accept`;
  });
  const denied = [...new Set([...hosts, '0.0.0.0/8', '10.0.0.0/8', '100.64.0.0/10',
    '127.0.0.0/8', '169.254.0.0/16', '172.16.0.0/12', '192.0.0.0/24', '192.0.2.0/24',
    '192.168.0.0/16', '198.18.0.0/15', '198.51.100.0/24', '203.0.113.0/24',
    '224.0.0.0/4', '240.0.0.0/4'])];
  return [
    'table inet stack_bench {',
    '  chain output {',
    '    type filter hook output priority -10; policy drop;',
    '    ct state established,related accept',
    '    oifname "lo" accept',
    ...allows,
    `    ip daddr { ${denied.join(', ')} } reject`,
    '    meta nfproto ipv4 tcp dport { 80, 443 } accept',
    '    reject',
    '  }',
    '}',
    '',
  ].join('\n');
}

export function dockerHostServiceAddress(networkMode: string = 'bridge'): string {
  if (networkMode === 'host' || /^container:[a-f0-9]{64}$/.test(networkMode)) return '127.0.0.1';
  if (networkMode === 'bridge') return DOCKER_HOST_ALIAS;
  throw new Error(`unsupported Docker network mode ${networkMode}`);
}

export function dockerHostGatewayArguments(networkMode: string = 'bridge'): string[] {
  if (networkMode === 'host' || /^container:[a-f0-9]{64}$/.test(networkMode)) return [];
  if (networkMode !== 'bridge') throw new Error(`unsupported Docker network mode ${networkMode}`);
  return ['--add-host', `${DOCKER_HOST_ALIAS}:host-gateway`];
}
