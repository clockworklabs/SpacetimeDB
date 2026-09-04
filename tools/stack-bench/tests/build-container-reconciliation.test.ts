import assert from 'node:assert/strict';
import type { SpawnSyncReturns } from 'node:child_process';
import test from 'node:test';

import { BUILD_CONTAINER_CREATION_LABEL, listRunningCodingContainers, removeFailedBuildContainer }
  from '../container/reconcile-build-container.js';

const ID = 'a'.repeat(64);
type DockerCall = [command: string, args: readonly string[]];

function dockerResult(stdout: string): SpawnSyncReturns<string> {
  return { pid: 0, output: [null, stdout, ''], stdout, stderr: '', status: 0, signal: null };
}

test('failed build-container cleanup removes the exact id returned by Docker', () => {
  const calls: DockerCall[] = [];
  const result = removeFailedBuildContainer({
    containerName: 'stack-bench-run', creationToken: 'token', createdId: `${ID}\n`,
    execute(command, args) {
      calls.push([command, args]);
      return dockerResult(ID);
    },
  });
  assert.deepEqual(calls, [['docker', ['rm', '-f', ID]]]);
  assert.deepEqual(result, { removed: true, absent: false, id: ID });
});

test('failed build-container cleanup reconciles a matching creation label', () => {
  const calls: DockerCall[] = [];
  removeFailedBuildContainer({
    containerName: 'stack-bench-run', creationToken: 'token',
    execute(command, args) {
      calls.push([command, args]);
      if (args[0] === 'inspect') {
        assert.equal(args[2], `{{.Id}} {{index .Config.Labels "${BUILD_CONTAINER_CREATION_LABEL}"}}`);
        return dockerResult(`${ID} token\n`);
      }
      return dockerResult(ID);
    },
  });
  assert.deepEqual(calls.map(([, args]) => args[0]), ['inspect', 'rm']);
  assert.deepEqual(calls[1], ['docker', ['rm', '-f', ID]]);
});

test('failed build-container cleanup never removes a same-name container with another label', () => {
  const calls: DockerCall[] = [];
  assert.throws(() => removeFailedBuildContainer({
    containerName: 'stack-bench-run', creationToken: 'token',
    execute(command, args) {
      calls.push([command, args]);
      return dockerResult(`${ID} another-token\n`);
    },
  }), /creation identity does not match/);
  assert.deepEqual(calls.map(([, args]) => args[0]), ['inspect']);
});

test('running coding containers are listed by their creation label only', () => {
  const calls: DockerCall[] = [];
  const names = listRunningCodingContainers({
    execute(command, args) {
      calls.push([command, args]);
      return dockerResult('stack-bench-build-b\nstack-bench-build-a\n\n');
    },
  });
  assert.deepEqual(calls, [['docker', ['ps', '--filter', `label=${BUILD_CONTAINER_CREATION_LABEL}`,
    '--format', '{{.Names}}']]]);
  assert.deepEqual(names, ['stack-bench-build-a', 'stack-bench-build-b']);
  assert.deepEqual(listRunningCodingContainers({ execute: () => dockerResult('') }), []);
  assert.throws(() => listRunningCodingContainers({ execute: () => ({
    ...dockerResult(''), status: 1, stderr: 'Cannot connect to the Docker daemon',
  }) }), /cannot list coding containers: Cannot connect to the Docker daemon/);
});
