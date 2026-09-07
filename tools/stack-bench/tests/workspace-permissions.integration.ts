import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { codingContainerWorkspaceHandoffCommands } from '../src/runtime/coding-container-policy.js';

test('capability-dropped controller and agent retain access through source restores and handoff', {
  skip: process.env.STACK_BENCH_WORKSPACE_PERMISSIONS_TEST !== '1', timeout: 60_000,
}, () => {
  const image = process.env.STACK_BENCH_NETWORK_CONTROLLER_IMAGE;
  assert.match(image ?? '', /^sha256:[a-f0-9]{64}$/);
  const prefix = `stack-bench-permissions-probe-${randomBytes(6).toString('hex')}`;
  const docker = (args: string[]): string => {
    const result = spawnSync('docker', args, { encoding: 'utf8', timeout: 30_000 });
    assert.equal(result.status, 0, `${args.slice(0, 3).join(' ')}: ${result.stderr || result.error}`);
    return result.stdout.trim();
  };
  const volume = docker(['volume', 'create', prefix]);
  const state = docker(['volume', 'inspect', '--format', '{{.Mountpoint}}', volume]);
  const containers: string[] = [];
  const create = (suffix: string, args: string[]): string => {
    const id = docker(['create', '--name', `${prefix}-${suffix}`, '--network', 'none', '--read-only',
      '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true', ...args,
      '--entrypoint', 'sleep', image!, 'infinity']);
    containers.push(id);
    docker(['start', id]);
    return id;
  };
  try {
    const controller = create('controller', ['--mount', `type=volume,source=${volume},target=/proof`,
      '--mount', `type=bind,source=${fileURLToPath(new URL('../', import.meta.url))},target=/opt/stack-bench/dist,readonly`]);
    const source = `import * as fs from 'node:fs';
      import {seedAppSource,restoreAppSource,resetAppToSource} from './dist/src/runtime/source-snapshot.js';`;
    docker(['exec', controller, 'node', '--input-type=module', '-e', source + `
      fs.mkdirSync('/proof/source/src',{recursive:true});
      fs.writeFileSync('/proof/source/src/app.js','original');
      fs.chmodSync('/proof/source/src/app.js',0o444);
      fs.chmodSync('/proof/source/src',0o555);
      seedAppSource('/proof/source','/proof/app');`]);
    const mount = ['--mount', `type=bind,source=${state}/app,target=/app`];
    const handoff = create('handoff', [...mount, '--cap-add', 'CHOWN', '--cap-add', 'FOWNER']);
    const agent = create('agent', [...mount, '--user', '10001:10001']);
    for (const operation of ['seed', 'restore', 'reset']) {
      if (operation !== 'seed') docker(['exec', controller, 'node', '--input-type=module', '-e', source
        + `${operation === 'restore' ? 'restoreAppSource' : 'resetAppToSource'}('/proof/source','/proof/app');`]);
      for (const command of codingContainerWorkspaceHandoffCommands(0)) docker(['exec', handoff, ...command]);
      docker(['exec', agent, 'node', '-e', `const fs=require('node:fs'),assert=require('node:assert/strict');
        assert.equal(fs.readFileSync('/app/src/app.js','utf8'),'original');
        fs.writeFileSync('/app/src/app.js','agent edit');
        fs.writeFileSync('/app/src/new.js','agent addition');
        assert.equal(fs.statSync('/app/src/app.js').mode & 0o007,0);`]);
      docker(['exec', controller, 'node', '-e', `const fs=require('node:fs'),assert=require('node:assert/strict');
        assert.equal(fs.readFileSync('/proof/app/src/app.js','utf8'),'agent edit');
        fs.writeFileSync('/proof/app/controller-note','controller can write');`]);
    }
  } finally {
    for (const id of containers.reverse()) docker(['rm', '-f', '--volumes', id]);
    docker(['volume', 'rm', volume]);
  }
});
