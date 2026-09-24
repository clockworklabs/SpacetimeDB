import assert from 'node:assert/strict';
import test from 'node:test';

import { referenceInstallSteps } from '../src/references/reference-install.js';

test('reference installs use clean locked installs and refresh only an unfrozen Spacetime SDK lock', () => {
  const clean = (directory: string) =>
    ({ directory, command: 'npm', args: ['ci', '--no-audit', '--no-fund'] });
  for (const [reference, steps] of [
    [{ kind: 'spacetime', frozenLock: true, installDirectories: ['backend/spacetimedb', 'client'],
      moduleDirectory: 'backend/spacetimedb' }, [clean('backend/spacetimedb'), clean('client')]],
    [{ kind: 'spacetime', installDirectories: ['client'] }, [
      { directory: 'client', command: 'npm',
        args: ['install', 'spacetimedb@file:/deps/spacetimedb.tgz', '--package-lock-only',
          '--ignore-scripts', '--no-audit', '--no-fund'] },
      clean('client')]],
    [{ kind: 'node-api', installDirectories: ['server', 'client'] }, [clean('server'), clean('client')]],
  ] as const) {
    assert.deepEqual(referenceInstallSteps(reference), steps, reference.kind);
  }
});
