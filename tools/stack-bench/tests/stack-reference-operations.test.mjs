import assert from 'node:assert/strict';
import test from 'node:test';

import { deploySpacetimeReference } from '../src/stacks/stack-reference-operations.mjs';

test('Spacetime reference client uses its assigned Vite port', async () => {
  const starts = [];
  const waits = [];
  const helpers = {
    docker() {},
    loadTrack() { return { id: 'ecommerce' }; },
    moduleName() { return 'ecommerce_42'; },
    startDetached(...args) { starts.push(args); },
    async waitFor(...args) { waits.push(args); },
    containerLogs() { return ''; },
  };

  await deploySpacetimeReference({
    args: { track: 'ecommerce', runIndex: 42 },
    metadata: {
      installDirectories: [],
      moduleDirectory: 'backend/spacetimedb',
      bindingsDirectory: 'client/src/module_bindings',
      client: { directory: 'client' },
    },
    lease: { resources: {
      module: 'ecommerce_42',
      serverUri: 'ws://host.docker.internal:3315',
    } },
    container: 'build-42',
    ports: { vite: 6475 },
    buildNetworkMode: 'bridge',
    helpers,
  });

  assert.equal(starts.length, 1);
  assert.deepEqual(starts[0].slice(0, 3), ['build-42', '/app/client', 'reference-client']);
  assert.equal(starts[0][3].VITE_PORT, '6475');
  assert.deepEqual(starts[0][4], { networkVisible: true, port: 6475 });
  assert.equal(waits[0][0], 'http://127.0.0.1:6475');
});
