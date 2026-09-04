import { spawnSync } from 'node:child_process';
import type { SpawnSyncOptionsWithStringEncoding, SpawnSyncReturns } from 'node:child_process';

export const BUILD_CONTAINER_CREATION_LABEL = 'com.clockworklabs.stack-bench.creation';

const CONTAINER_ID = /^[a-f0-9]{64}$/i;

type DockerResult = SpawnSyncReturns<string>;
type DockerExecute = (command: string, args: readonly string[],
  options: SpawnSyncOptionsWithStringEncoding) => DockerResult;

export interface RemoveFailedBuildContainerOptions {
  containerName: string;
  creationToken: string;
  createdId?: string | null;
  dockerEnv?: NodeJS.ProcessEnv;
  timeoutMs?: number;
  execute?: DockerExecute;
}

export function containerIdFromDockerOutput(output: unknown): string | null {
  return String(output ?? '').split(/\r?\n/).map(line => line.trim())
    .find(line => CONTAINER_ID.test(line)) ?? null;
}

function detail(result: DockerResult): string {
  return String(result.stderr || result.stdout || result.error?.message
    || `exit ${result.status}`).trim();
}

// The coding containers running on this Docker host, from any campaign. A
// stopped container keeps its label but holds no application or session.
export function listRunningCodingContainers({ dockerEnv = process.env, timeoutMs = 30_000,
  execute = spawnSync as DockerExecute }: {
    dockerEnv?: NodeJS.ProcessEnv; timeoutMs?: number; execute?: DockerExecute;
  } = {}): string[] {
  const result = execute('docker', ['ps', '--filter', `label=${BUILD_CONTAINER_CREATION_LABEL}`,
    '--format', '{{.Names}}'], { encoding: 'utf8', env: dockerEnv, timeout: timeoutMs });
  if (result.error || result.status !== 0) {
    throw new Error(`cannot list coding containers: ${detail(result)}`);
  }
  return String(result.stdout ?? '').split(/\r?\n/).map(line => line.trim())
    .filter(Boolean).sort();
}

export function removeFailedBuildContainer({ containerName, creationToken, createdId = null,
  dockerEnv = process.env, timeoutMs = 120_000,
  execute = spawnSync as DockerExecute }: RemoveFailedBuildContainerOptions): {
  removed: boolean; absent: boolean; id?: string;
} {
  if (typeof containerName !== 'string' || !containerName) {
    throw new Error('failed build-container cleanup requires a container name');
  }
  if (typeof creationToken !== 'string' || !creationToken) {
    throw new Error('failed build-container cleanup requires a creation token');
  }

  let id = containerIdFromDockerOutput(createdId);
  if (!id) {
    const inspected = execute('docker', ['inspect', '--format',
      `{{.Id}} {{index .Config.Labels "${BUILD_CONTAINER_CREATION_LABEL}"}}`, containerName], {
      encoding: 'utf8', env: dockerEnv, timeout: timeoutMs,
    });
    if (inspected.status !== 0) {
      const reason = detail(inspected);
      if (/no such (?:object|container)/i.test(reason)) return { removed: false, absent: true };
      throw new Error(`cannot prove cleanup of failed container ${containerName}: ${reason}`);
    }
    const [inspectedId, label, ...extra] = String(inspected.stdout ?? '').trim().split(/\s+/);
    if (!CONTAINER_ID.test(inspectedId ?? '') || label !== creationToken || extra.length > 0) {
      throw new Error(`refusing to remove ${containerName}: its creation identity does not match`);
    }
    id = inspectedId ?? null;
  }

  if (!id) throw new Error(`cannot prove cleanup of failed container ${containerName}`);

  const removed = execute('docker', ['rm', '-f', id], {
    encoding: 'utf8', env: dockerEnv, timeout: timeoutMs,
  });
  if (removed.status !== 0) {
    throw new Error(`could not remove failed build container ${id}: ${detail(removed)}`);
  }
  return { removed: true, absent: false, id };
}
