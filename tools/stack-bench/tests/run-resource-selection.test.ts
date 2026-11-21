import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { loadTrack, portsFor } from '../src/composition/tracks.js';
import { runResourceLockKeys } from '../src/runtime/backend-lease.js';
import { selectRunResources } from '../src/runtime/run-resource-selection.js';

test('resource selection skips exclusions, held locks and occupied ports across every requested stack', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-selection-'));
  try {
    const track = loadTrack('ecommerce');
    const held = 'slot:ecommerce:mongodb:run1';
    writeFileSync(join(root, `${createHash('sha256').update(held).digest('hex')}.lock.json`), '{}');
    const busy = portsFor(track, 'postgres', 2).express;
    const backends = ['postgres', 'mongodb'];
    const result = await selectRunResources({ track, backends, count: 2,
      serverUri: () => null, excludedRunIndices: [0],
      env: { STACK_BENCH_RESOURCE_LOCK_DIR: root }, probePort: port => ({ free: port !== busy }) });
    assert.deepEqual(result.runIndices, [3, 4]);
    assert.deepEqual(result.keys, result.runIndices.flatMap(runIndex => backends.flatMap(backend =>
      runResourceLockKeys({ track: track.name, backend, runIndex, ports: portsFor(track, backend, runIndex) }))));
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('resource selection probes the database listener and keeps selected groups disjoint', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-selection-'));
  try {
    const result = await selectRunResources({ track: loadTrack('ecommerce'), backends: ['spacetime'], count: 2,
      // Runs 1 and 2 cannot both own the same database listener.
      serverUri: index => `http://127.0.0.1:${index < 3 ? 18000 + Math.min(index, 1) : 18000 + index}`,
      env: { STACK_BENCH_RESOURCE_LOCK_DIR: root }, probePort: port => ({ free: port !== 18000 }) });
    assert.deepEqual(result.runIndices, [1, 3]);
  } finally { rmSync(root, { recursive: true, force: true }); }
  const mixedRoot = mkdtempSync(join(tmpdir(), 'stack-bench-selection-'));
  try {
    const mixed = await selectRunResources({ track: loadTrack('ecommerce'),
      backends: ['spacetime', 'convex'], count: 1,
      serverUri: (index, backend) => `http://127.0.0.1:${(backend === 'convex' ? 19000 : 18000) + index}`,
      env: { STACK_BENCH_RESOURCE_LOCK_DIR: mixedRoot }, probePort: port => ({ free: port !== 19000 }) });
    assert.deepEqual(mixed.runIndices, [1]);
    assert(mixed.keys.includes('port:18001'));
    assert(mixed.keys.includes('port:19001'));
  } finally { rmSync(mixedRoot, { recursive: true, force: true }); }
});

test('resource selection aborts before probing on cancellation', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-selection-'));
  try {
    const input = { track: loadTrack('ecommerce'), backends: ['postgres'], count: 1,
      serverUri: () => null, env: { STACK_BENCH_RESOURCE_LOCK_DIR: root },
      probePort: () => ({ free: true }) };
    await assert.rejects(selectRunResources({ ...input, signal: AbortSignal.abort(),
      probePort: () => { throw new Error('must not probe'); } }), { name: 'AbortError' });
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('appliance admission skips ports Docker cannot publish and stops on other Docker errors', {
  skip: process.platform !== 'linux' && 'uses a POSIX docker stub',
}, async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-selection-'));
  const bin = mkdtempSync(join(tmpdir(), 'stack-bench-docker-'));
  try {
    const track = loadTrack('ecommerce');
    const busy = portsFor(track, 'postgres', 0).vite;
    // Docker Desktop reports host ports the Linux loopback probe cannot see.
    writeFileSync(join(bin, 'docker'), [
      '#!/bin/sh',
      '[ -n "$DOCKER_STUB_ERROR" ] && { echo "$DOCKER_STUB_ERROR" >&2; exit 125; }',
      `case " $* " in *"127.0.0.1:${busy}:${busy}"*) echo "port is already allocated" >&2; exit 125;; esac`,
      'exit 0', '',
    ].join('\n'), { mode: 0o755 });
    const env = { STACK_BENCH_RESOURCE_LOCK_DIR: root, STACK_BENCH_APPLIANCE: '1',
      STACK_BENCH_CONTROLLER_IMAGE_ID: `sha256:${'a'.repeat(64)}`, PATH: `${bin}:${process.env.PATH}` };
    const input = { track, backends: ['postgres'], count: 1, serverUri: () => null,
      probePort: () => ({ free: true }) };
    assert.deepEqual((await selectRunResources({ ...input, env })).runIndices, [1]);
    await assert.rejects(selectRunResources({ ...input, env: { ...env, DOCKER_STUB_ERROR: 'Unable to find image' } }),
      /Command failed/);
  } finally {
    rmSync(root, { recursive: true, force: true });
    rmSync(bin, { recursive: true, force: true });
  }
});
