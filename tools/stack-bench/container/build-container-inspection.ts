import { spawnSync } from 'node:child_process';
import type { SpawnSyncOptionsWithStringEncoding, SpawnSyncReturns } from 'node:child_process';
import { resolve } from 'node:path';

import { LEGACY_SUBSCRIPTION_TOKEN_TARGET } from './container-auth.js';
import type { ContainerMount } from '../src/runtime/container-mount.js';

type InspectedMount = {
  type: string;
  source: string;
  name: string | null;
  destination: string;
  readOnly: boolean;
};

export type InspectedBuildContainer = {
  id: string;
  image: string;
  running: boolean;
  networkMode: string | null;
  readonlyRootfs: boolean;
  tmpfs: Record<string, string>;
  capAdd: string[];
  capDrop: string[];
  securityOpt: string[];
  pidsLimit: number | null;
  nanoCpus: number | null;
  memoryBytes: number | null;
  memorySwapBytes: number | null;
  mounts: InspectedMount[];
  unsafeCredentialExposure: boolean;
};

type DockerExecute = (command: string, args: readonly string[],
  options: SpawnSyncOptionsWithStringEncoding) => SpawnSyncReturns<string>;

type JsonRecord = Record<string, unknown>;

function isRecord(value: unknown): value is JsonRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

function numberOrNull(value: unknown): number | null {
  return typeof value === 'number' ? value : null;
}

function dockerDetail(result: SpawnSyncReturns<string>): string {
  return String(result.stderr || result.stdout || result.error?.message || `exit ${result.status}`).trim();
}

export function parseCgroupMemory(value: string) {
  const bytes = (name: string): number | null => {
    const match = value.match(new RegExp(`^\\[${name.replace('.', '\\.')}]\\r?\\n(\\d+)$`, 'm'));
    if (!match) return null;
    const parsed = Number(match[1]);
    return Number.isSafeInteger(parsed) ? parsed : null;
  };
  return {
    currentBytes: bytes('memory.current'),
    peakBytes: bytes('memory.peak'),
    limitBytes: bytes('memory.max'),
  };
}

export function inspectBuildContainer(name: string, {
  env = process.env,
  timeoutMs = 120_000,
  execute = spawnSync as DockerExecute,
}: { env?: NodeJS.ProcessEnv; timeoutMs?: number; execute?: DockerExecute } = {}): InspectedBuildContainer | null {
  const result = execute('docker', ['inspect', name], { encoding: 'utf8', env, timeout: timeoutMs });
  if (result.status !== 0) {
    const detail = dockerDetail(result);
    if (/no such (?:object|container)/i.test(detail)) return null;
    throw new Error(`cannot inspect build container ${name}: ${detail}`);
  }

  let parsed: unknown;
  try { parsed = JSON.parse(result.stdout); }
  catch (error) {
    throw new Error(`Docker returned invalid inspection JSON for ${name}: ${error instanceof Error
      ? error.message : String(error)}`);
  }
  if (!Array.isArray(parsed) || !isRecord(parsed[0])) {
    throw new Error(`Docker returned an invalid container inspection for ${name}`);
  }

  const inspected = parsed[0];
  const mounts = Array.isArray(inspected.Mounts) ? inspected.Mounts.filter(isRecord) : [];
  const config = isRecord(inspected.Config) ? inspected.Config : {};
  const hostConfig = isRecord(inspected.HostConfig) ? inspected.HostConfig : {};
  const state = isRecord(inspected.State) ? inspected.State : {};
  const sensitiveTargets = new Set([LEGACY_SUBSCRIPTION_TOKEN_TARGET, '/root/.claude/.credentials.json']);
  const capabilities = (values: unknown): string[] => stringArray(values).map(value => value.replace(/^CAP_/, ''));
  const tmpfs = isRecord(hostConfig.Tmpfs)
    ? Object.fromEntries(Object.entries(hostConfig.Tmpfs)
      .filter((entry): entry is [string, string] => typeof entry[1] === 'string')) : {};

  return {
    id: String(inspected.Id),
    image: String(inspected.Image),
    running: state.Running === true,
    networkMode: typeof hostConfig.NetworkMode === 'string' ? hostConfig.NetworkMode : null,
    readonlyRootfs: hostConfig.ReadonlyRootfs === true,
    tmpfs,
    capAdd: capabilities(hostConfig.CapAdd),
    capDrop: capabilities(hostConfig.CapDrop),
    securityOpt: stringArray(hostConfig.SecurityOpt).map(option => option.replace(/:true$/, '')),
    pidsLimit: numberOrNull(hostConfig.PidsLimit),
    nanoCpus: numberOrNull(hostConfig.NanoCpus),
    memoryBytes: numberOrNull(hostConfig.Memory),
    memorySwapBytes: numberOrNull(hostConfig.MemorySwap),
    mounts: mounts.map(mount => ({
      type: String(mount.Type),
      source: String(mount.Source),
      name: typeof mount.Name === 'string' ? mount.Name : null,
      destination: String(mount.Destination),
      readOnly: mount.RW !== true,
    })),
    unsafeCredentialExposure: mounts.some(mount => sensitiveTargets.has(String(mount.Destination)))
      || stringArray(config.Env).some(value => /^(?:ANTHROPIC_API_KEY|CLAUDE_CODE_OAUTH_TOKEN)=/.test(value)),
  };
}

export function sameHostPath(left: string, right: string,
  platform: NodeJS.Platform = process.platform): boolean {
  const normalize = (value: string): string => resolve(value).replaceAll('\\', '/');
  const normalizedLeft = normalize(left);
  const normalizedRight = normalize(right);
  return platform === 'win32'
    ? normalizedLeft.toLowerCase() === normalizedRight.toLowerCase()
    : normalizedLeft === normalizedRight;
}

export function parsePublishedPorts(value: string | undefined): string[] {
  if (!value) return [];
  const ports = value.split(',').map(port => port.trim()).filter(Boolean);
  if (ports.some(port => !/^\d+$/.test(port) || Number(port) < 1 || Number(port) > 65_535)) {
    throw new Error('--ports must contain integers from 1 through 65535');
  }
  if (new Set(ports).size !== ports.length) throw new Error('--ports must not contain duplicates');
  return ports;
}

export function hasRequiredBuildContainerIsolation(container: InspectedBuildContainer, {
  expectedMounts,
  requiredTmpfs,
  requiredCapabilities,
  pidsLimit,
  cpuCount,
  memoryBytes,
  memorySwapBytes,
  image,
}: {
  expectedMounts: ContainerMount[];
  requiredTmpfs: Readonly<Record<string, string>>;
  requiredCapabilities: readonly string[];
  pidsLimit: number;
  cpuCount: number;
  memoryBytes: number;
  memorySwapBytes: number;
  image: string;
}): boolean {
  const mountsMatch = container.mounts.length === expectedMounts.length
    && expectedMounts.every(expected => container.mounts.some(actual =>
      actual.type === (expected.kind ?? 'bind')
      && actual.destination === expected.target
      && actual.readOnly === expected.readOnly
      && (expected.kind === 'volume'
        ? actual.name === expected.source
        : sameHostPath(actual.source, expected.source))));
  return container.readonlyRootfs
    && Object.entries(requiredTmpfs).every(([path, options]) => container.tmpfs[path] === options)
    && Object.keys(container.tmpfs).length === Object.keys(requiredTmpfs).length
    && requiredCapabilities.every(capability => container.capAdd.includes(capability))
    && container.capAdd.length === requiredCapabilities.length
    && container.capDrop.includes('ALL')
    && container.securityOpt.includes('no-new-privileges')
    && container.pidsLimit === pidsLimit
    && container.nanoCpus === cpuCount * 1_000_000_000
    && container.memoryBytes === memoryBytes
    && container.memorySwapBytes === memorySwapBytes
    && container.image === image
    && mountsMatch;
}
