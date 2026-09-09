import assert from 'node:assert/strict';
import test from 'node:test';

import { RESTRICTED_PORTS, RUN_INDEX_CAP, listTracks, loadTrack,
  portsFor } from '../src/composition/tracks.js';
import { STACK_ADAPTER_REGISTRY } from '../src/stacks/stack-adapters.js';

test('dynamic indices validate actual ports, including browser restrictions and TCP bounds', () => {
  for (const name of listTracks({ includeInternal: true })) {
    const track = loadTrack(name);
    for (const backend of STACK_ADAPTER_REGISTRY.ids) {
      const ports = portsFor(track, backend, 30);
      for (const port of [ports.vite, ports.express]) {
        if (port != null) assert(!RESTRICTED_PORTS.has(port));
      }
      assert.throws(() => portsFor(track, backend, RUN_INDEX_CAP), /TCP port/);
    }
  }
  assert.throws(() => portsFor(loadTrack('chat'), 'mongodb', 256), /6679/);
});

test('the restricted list is the one fetch enforces', async () => {
  // The MongoDB ecommerce window once covered 6679; fetch reports it as a bad port
  // before any connection is made.
  assert(RESTRICTED_PORTS.has(6679));
  await assert.rejects(fetch('http://127.0.0.1:6679/', { signal: AbortSignal.timeout(2000) }),
    (error: unknown) => error instanceof TypeError && /fetch failed/.test(error.message));
});
