import { setImmediate as yieldTurn } from 'node:timers/promises';
import { portsFor, RUN_INDEX_CAP, type Track } from '../composition/tracks.js';
import { existingResourceLockKeys, loopbackHttpUri, resourceLockScope,
  runResourceLockKeys } from './backend-lease.js';

// Selection is a hint. The caller must atomically claim the returned resources
// and repeat selection if another owner wins the claim.
export async function selectRunResources(input: {
  track: Track;
  backends: readonly string[];
  count: number;
  serverUri: (runIndex: number) => string | null;
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
      const uri = serverUri(runIndex);
      ports = new Set(backends.flatMap(backend => {
        const assigned = portsFor(track, backend, runIndex);
        return [assigned.vite, assigned.express].filter((port): port is number => typeof port === 'number');
      }));
      if (backends.includes('spacetime') && uri) ports.add(Number(loopbackHttpUri(uri).port));
      candidateKeys = backends.flatMap(backend => runResourceLockKeys({
        track: track.name, backend, runIndex, ports: portsFor(track, backend, runIndex),
        serverUri: backend === 'spacetime' ? uri : null,
      }));
    } catch (error) {
      if (error instanceof RangeError) continue;
      throw error;
    }
    if (candidateKeys.some(key => selectedKeys.has(key))
      || existingResourceLockKeys({ ...scope, keys: candidateKeys }).length
      || ![...ports].every(port => probePort(port).free)) continue;
    runIndices.push(runIndex);
    keys.push(...candidateKeys);
    for (const key of candidateKeys) selectedKeys.add(key);
    if (runIndices.length === count) break;
  }
  return { runIndices, keys };
}
