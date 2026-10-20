import { RESTRICTED_PORTS } from '../src/composition/product-config.js';
import assert from 'node:assert/strict';
import test from 'node:test';

import { RUN_INDEX_CAP, listTracks, loadTrack,
  portsFor } from '../src/composition/tracks.js';
import { STACK_IDS } from '../src/stacks/stack-identities.js';

test('dynamic indices validate actual ports, including browser restrictions and TCP bounds', () => {
  for (const name of listTracks({ includeInternal: true })) {
    const track = loadTrack(name);
    for (const backend of STACK_IDS) {
      const ports = portsFor(track, backend, 30);
      for (const port of [ports.vite, ports.express]) {
        if (port != null) assert(!RESTRICTED_PORTS.has(port));
      }
      assert.throws(() => portsFor(track, backend, RUN_INDEX_CAP), /TCP port/);
    }
  }
  assert.throws(() => portsFor(loadTrack('chat'), 'mongodb', 256), /6679/);
});

test('Supabase application ports stay clear of every other stack and of its platform services', () => {
  const tracks = listTracks({ includeInternal: true }).map(name => loadTrack(name));
  // Indices whose ports a browser refuses are never assigned.
  const assigned = (track: ReturnType<typeof loadTrack>, backend: string, runIndex: number) => {
    try { return portsFor(track, backend, runIndex); } catch { return null; }
  };
  const window = (backend: string) => new Set(tracks.flatMap(track => Array.from({ length: 64 }, (_, runIndex) => {
    const ports = assigned(track, backend, runIndex);
    return ports ? [ports.vite, ports.express, ports.dbPort].filter((port): port is number => port !== null) : [];
  }).flat()));
  const supabase = window('supabase');
  for (const backend of STACK_IDS.filter(id => id !== 'supabase')) {
    assert.deepEqual([...window(backend)].filter(port => supabase.has(port)), [], backend);
  }
  // Services listening inside the attempt namespace, and the appliance dashboard.
  for (const port of [3000, 3001, 4000, 5000, 5432, 7331, 9000, 9901, 9999]) assert(!supabase.has(port), String(port));
});
