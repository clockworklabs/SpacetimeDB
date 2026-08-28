import { spawnSync } from 'node:child_process';

export const BUILD_CONTAINER_CREATION_LABEL = 'com.clockworklabs.stack-bench.creation';

const CONTAINER_ID = /^[a-f0-9]{64}$/i;

export function containerIdFromDockerOutput(output) {
  return String(output ?? '').split(/\r?\n/).map(line => line.trim())
    .find(line => CONTAINER_ID.test(line)) ?? null;
}

function detail(result) {
  return String(result.stderr || result.stdout || result.error?.message
    || `exit ${result.status}`).trim();
}

export function removeFailedBuildContainer({ containerName, creationToken, createdId = null,
  dockerEnv = process.env, timeoutMs = 120_000, execute = spawnSync }) {
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
    id = inspectedId;
  }

  const removed = execute('docker', ['rm', '-f', id], {
    encoding: 'utf8', env: dockerEnv, timeout: timeoutMs,
  });
  if (removed.status !== 0) {
    throw new Error(`could not remove failed build container ${id}: ${detail(removed)}`);
  }
  return { removed: true, absent: false, id };
}
