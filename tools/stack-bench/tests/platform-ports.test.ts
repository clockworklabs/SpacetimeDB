import test from 'node:test';
import assert from 'node:assert/strict';
import childProcess from 'node:child_process';
import { syncBuiltinESMExports } from 'node:module';

test('port probes retain strict failure handling through the shared Linux fallback', async t => {
  const platform = Object.getOwnPropertyDescriptor(process, 'platform')!;
  let probe: typeof import('../src/runtime/platform.js');
  try {
    // Exercise the Linux branch on Windows without running host commands.
    Object.defineProperty(process, 'platform', { value: 'linux' });
    probe = await import(new URL('../src/runtime/platform.js?port-test', import.meta.url).href);
  } finally {
    Object.defineProperty(process, 'platform', platform);
  }
  const failed = (status: number, stdout = '') => Object.assign(new Error('probe failed'), { status, stdout });
  let lsof: string | Error;
  let ss: string | Error;
  const calls: string[] = [];
  const command = t.mock.method(childProcess, 'execFileSync', (name: string) => {
    calls.push(name);
    assert.ok(name === 'lsof' || name === 'ss');
    const result = name === 'lsof' ? lsof : ss;
    if (result instanceof Error) throw result;
    return result;
  });
  syncBuiltinESMExports();
  try {
    lsof = failed(2, '999\n');
    ss = 'LISTEN 0 10 [::]:7331 [::]:* users:(("app",pid=42,fd=3))\n'
      + 'LISTEN 0 10 0.0.0.0:7332 0.0.0.0:* users:(("other",pid=99,fd=3))';
    assert.deepEqual(probe.pidsOnPort(7331, { strict: true }), ['42']);
    assert.deepEqual(calls, ['lsof', 'ss']);
    ss = failed(127);
    assert.throws(() => probe.pidsOnPort(7331, { strict: true }), /could not inspect listeners/);
    assert.deepEqual(probe.pidsOnPort(7331), ['999']);
    lsof = failed(1);
    assert.deepEqual(probe.pidsOnPort(7331, { strict: true }), []);
    lsof = '42\n42\n';
    calls.length = 0;
    assert.deepEqual(probe.pidsOnPort(7331, { strict: true }), ['42']);
    assert.deepEqual(calls, ['lsof']);
  } finally {
    command.mock.restore();
    syncBuiltinESMExports();
  }
});
