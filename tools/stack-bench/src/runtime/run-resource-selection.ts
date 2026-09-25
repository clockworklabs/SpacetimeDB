import { setImmediate as yieldTurn } from 'node:timers/promises';
import { execFileSync } from 'node:child_process';
import { portsFor, RUN_INDEX_CAP, type Track } from '../composition/tracks.js';
import { isExactImageReference, isImageId } from './container-image.js';
import { existingResourceLockKeys, loopbackHttpUri, resourceLockScope,
  runResourceLockKeys } from './backend-lease.js';

// Docker Desktop publishes ports on the desktop host, which the Linux loopback
// probe cannot see. Let Docker test the whole candidate before reserving it.
function dockerCanPublish(ports: Set<number>, env: NodeJS.ProcessEnv): boolean {
  if (env.STACK_BENCH_APPLIANCE !== '1') return true;
  const image = env.STACK_BENCH_CONTROLLER_IMAGE_ID ?? env.STACK_BENCH_CONTROLLER_IMAGE;
  if (!isImageId(image) && !isExactImageReference(image)) {
    throw new Error('port admission requires the pinned controller image');
  }
  try {
    execFileSync('docker', ['run', '--rm', '--pull', 'never',
      ...[...ports].flatMap(port => ['--publish', `127.0.0.1:${port}:${port}`]),
      '--entrypoint', '/bin/true', image],
    { encoding: 'utf8', stdio: 'pipe', timeout: 30_000, env });
    return true;
  } catch (error) {
    const detail = error && typeof error === 'object' && 'stderr' in error ? String(error.stderr) : '';
    if (/ports are not available|port is already allocated|address already in use/i.test(detail)) return false;
    throw error;
  }
}

// Selection is a hint. The caller must atomically claim the returned resources
// and repeat selection if another owner wins the claim.
export async function selectRunResources(input: {
  track: Track;
  backends: readonly string[];
  count: number;
  serverUri: (runIndex: number, backend: string) => string | null;
  probePort: (port: number | string) => { free: boolean };
  env?: NodeJS.ProcessEnv;
  excludedRunIndices?: readonly number[];
  signal?: AbortSignal;
}): Promise<{ runIndices: number[]; keys: string[] }> {
  const { track, backends, count, serverUri, probePort, signal } = input;
  if (!Number.isInteger(count) || count < 1 || count > RUN_INDEX_CAP + 1
    || backends.length === 0) throw new Error('invalid run resource selection');
  const scope = resourceLockScope(input.env);
  const runIndices: number[] = [], keys: string[] = [];
  const selectedKeys = new Set<string>();
  for (let runIndex = 0; runIndex <= RUN_INDEX_CAP; runIndex += 1) {
    if (runIndex % 16 === 0) await yieldTurn(undefined, { signal });
    if (input.excludedRunIndices?.includes(runIndex)) continue;
    let candidateKeys: string[];
    let ports: Set<number>;
    try {
      ports = new Set(backends.flatMap(backend => {
        const assigned = portsFor(track, backend, runIndex);
        const uri = serverUri(runIndex, backend);
        return [assigned.vite, assigned.express, uri ? Number(loopbackHttpUri(uri).port) : null]
          .filter((port): port is number => typeof port === 'number');
      }));
      candidateKeys = backends.flatMap(backend => runResourceLockKeys({
        track: track.name, backend, runIndex, ports: portsFor(track, backend, runIndex),
        serverUri: serverUri(runIndex, backend),
      }));
    } catch (error) {
      if (error instanceof RangeError) continue;
      throw error;
    }
    if (candidateKeys.some(key => selectedKeys.has(key))
      || existingResourceLockKeys({ ...scope, keys: candidateKeys }).length
      || ![...ports].every(port => probePort(port).free)
      || !dockerCanPublish(ports, input.env ?? process.env)) continue;
    runIndices.push(runIndex);
    keys.push(...candidateKeys);
    for (const key of candidateKeys) selectedKeys.add(key);
    if (runIndices.length === count) break;
  }
  return { runIndices, keys };
}
