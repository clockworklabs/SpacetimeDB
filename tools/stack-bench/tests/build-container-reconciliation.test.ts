import assert from 'node:assert/strict';
import type { SpawnSyncReturns } from 'node:child_process';
import test from 'node:test';

import { BUILD_CONTAINER_CREATION_LABEL, buildContainerName, removeFailedBuildContainer }
  from '../container/reconcile-build-container.js';

const ID = 'a'.repeat(64);

test('build container names are stable per lease and independent of app directory parents', () => {
  const first = { runId: 'first-run', resources: {} };
  const second = { runId: 'second-run', resources: {} };
  assert.notEqual(buildContainerName(first), buildContainerName(second));
  assert.equal(buildContainerName(first), buildContainerName(first));
  assert.match(buildContainerName(first), /^sb-[a-f0-9]{16}-build$/);
  assert.equal(buildContainerName({ ...first, resources: {
    buildContainer: { name: 'existing-leased-container' },
  } }), 'existing-leased-container');
});
type DockerCall = [command: string, args: readonly string[]];

function dockerResult(stdout: string): SpawnSyncReturns<string> {
  return { pid: 0, output: [null, stdout, ''], stdout, stderr: '', status: 0, signal: null };
}

test('failed build-container cleanup reconciles a matching creation label', () => {
  for (const createdId of [null, `${ID}\n`]) {
    const calls: DockerCall[] = [];
    const result = removeFailedBuildContainer({
      containerName: 'stack-bench-run', creationToken: 'token', createdId,
      execute(command, args) {
        calls.push([command, args]);
        if (args[0] === 'inspect') {
          assert.equal(args[2], `{{.Id}} {{index .Config.Labels "${BUILD_CONTAINER_CREATION_LABEL}"}}`);
          return dockerResult(`${ID} token\n`);
        }
        return dockerResult(ID);
      },
    });
    assert.deepEqual(calls.map(([, args]) => args[0]), createdId ? ['rm'] : ['inspect', 'rm']);
    assert.deepEqual(calls.at(-1), ['docker', ['rm', '-f', ID]]);
    assert.deepEqual(result, { removed: true, absent: false, id: ID });
  }
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
