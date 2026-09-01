import assert from 'node:assert/strict';
import test from 'node:test';

import { inspectBuildContainer, parsePublishedPorts, sameHostPath }
  from '../container/build-container-inspection.js';

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
