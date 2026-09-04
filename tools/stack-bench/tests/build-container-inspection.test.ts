import assert from 'node:assert/strict';
import test from 'node:test';

import { hasRequiredBuildContainerIsolation, inspectBuildContainer, parseCgroupMemory,
  parsePublishedPorts, sameHostPath }
  from '../container/build-container-inspection.js';
import type { InspectedBuildContainer } from '../container/build-container-inspection.js';

const dockerResult = (status: number, stdout = '', stderr = '') => ({
  status, stdout, stderr, pid: 1, output: [], signal: null,
});

test('build-container inspection distinguishes absence from Docker failure', () => {
  assert.equal(inspectBuildContainer('missing', {
    execute: () => dockerResult(1, '', 'Error: No such object: missing'),
  }), null);
  assert.throws(() => inspectBuildContainer('leased', {
    execute: () => dockerResult(1, '', 'Cannot connect to the Docker daemon'),
  }), /cannot inspect build container leased/);
});

test('host path comparison preserves Linux case sensitivity', () => {
  assert.equal(sameHostPath('/work/App', '/work/app', 'linux'), false);
  assert.equal(sameHostPath('C:\\work\\App', 'c:\\work\\app', 'win32'), true);
});

test('published ports must be valid and unique', () => {
  assert.deepEqual(parsePublishedPorts('3000, 5173'), ['3000', '5173']);
  assert.throws(() => parsePublishedPorts('3000,3000'), /must not contain duplicates/);
  assert.throws(() => parsePublishedPorts('0'), /integers from 1 through 65535/);
  assert.throws(() => parsePublishedPorts('3000x'), /integers from 1 through 65535/);
});

test('cgroup memory reports current use, peak use, and the enforced limit', () => {
  assert.deepEqual(parseCgroupMemory('[memory.events]\noom 0\n[memory.current]\n123\n'
    + '[memory.peak]\n456\n[memory.max]\n4294967296\n'), {
    currentBytes: 123,
    peakBytes: 456,
    limitBytes: 4 * 1024 ** 3,
  });
  assert.deepEqual(parseCgroupMemory('[memory.max]\nmax\n'), {
    currentBytes: null,
    peakBytes: null,
    limitBytes: null,
  });
});

test('build-container isolation validates the effective Docker configuration', () => {
  const container: InspectedBuildContainer = {
    id: 'container',
    image: 'image',
    running: true,
    networkMode: 'bridge',
    readonlyRootfs: true,
    tmpfs: { '/tmp': 'rw,nosuid,nodev' },
    capAdd: ['CHOWN'],
    capDrop: ['ALL'],
    securityOpt: ['no-new-privileges'],
    pidsLimit: 32,
    nanoCpus: 2_000_000_000,
    memoryBytes: 1024,
    memorySwapBytes: 1024,
    mounts: [{ type: 'bind', source: '/workspace', name: null,
      destination: '/app', readOnly: false }],
    unsafeCredentialExposure: false,
  };
  const requirements = {
    expectedMounts: [{ kind: 'bind' as const, source: '/workspace', target: '/app', readOnly: false }],
    requiredTmpfs: { '/tmp': 'rw,nosuid,nodev' },
    requiredCapabilities: ['CHOWN'],
    pidsLimit: 32,
    cpuCount: 2,
    memoryBytes: 1024,
    memorySwapBytes: 1024,
    image: 'image',
  };

  assert.equal(hasRequiredBuildContainerIsolation(container, requirements), true);
  assert.equal(hasRequiredBuildContainerIsolation({ ...container, readonlyRootfs: false }, requirements), false);
  assert.equal(hasRequiredBuildContainerIsolation({ ...container, capDrop: [] }, requirements), false);
  assert.equal(hasRequiredBuildContainerIsolation({ ...container, memoryBytes: 2048 }, requirements), false);
});
