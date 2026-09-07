import assert from 'node:assert/strict';
import test from 'node:test';

import { RESTRICTED_PORTS, RUN_INDEX_CAP, assertNoPortCollisions, listTracks, loadTrack,
  portsFor } from '../src/composition/tracks.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';

test('no run window contains a port browsers refuse', () => {
  assert.doesNotThrow(() => assertNoPortCollisions());
  for (const name of listTracks({ includeInternal: true })) {
    const track = loadTrack(name);
    for (const backend of STACK_ADAPTER_REGISTRY.ids) {
      for (let index = 0; index <= RUN_INDEX_CAP; index++) {
        const ports = portsFor(track, backend, index);
        for (const port of [ports.vite, ports.express]) {
          if (port != null) assert(!RESTRICTED_PORTS.has(port), `${name}/${backend}/run${index} leases ${port}`);
        }
      }
    }
  }
});

test('the restricted list is the one fetch enforces', async () => {
  // The MongoDB ecommerce window once covered 6679; fetch reports it as a bad port
  // before any connection is made.
  assert(RESTRICTED_PORTS.has(6679));
  await assert.rejects(fetch('http://127.0.0.1:6679/', { signal: AbortSignal.timeout(2000) }),
    (error: unknown) => error instanceof TypeError && /fetch failed/.test(error.message));
});
