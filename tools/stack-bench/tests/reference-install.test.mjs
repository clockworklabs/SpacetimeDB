import assert from 'node:assert/strict';
import test from 'node:test';

import { referenceInstallSteps } from '../src/references/reference-install.mjs';

test('Spacetime references refresh the release SDK lock before a clean install', () => {
  assert.deepEqual(referenceInstallSteps({
    kind: 'spacetime',
    installDirectories: ['backend/spacetimedb', 'client'],
    moduleDirectory: 'backend/spacetimedb',
  }), [
    { directory: 'backend/spacetimedb', command: 'npm',
      args: ['install', '--package-lock-only', '--ignore-scripts', '--no-audit', '--no-fund'] },
    { directory: 'backend/spacetimedb', command: 'npm', args: ['ci', '--no-audit', '--no-fund'] },
    { directory: 'client', command: 'npm',
      args: ['install', '--package-lock-only', '--ignore-scripts', '--no-audit', '--no-fund'] },
    { directory: 'client', command: 'npm', args: ['ci', '--no-audit', '--no-fund'] },
  ]);
});

test('hosted references retain clean installs without lock rewriting', () => {
  assert.deepEqual(referenceInstallSteps({
    kind: 'node-api', installDirectories: ['server', 'client'],
  }), [
    { directory: 'server', command: 'npm', args: ['ci', '--no-audit', '--no-fund'] },
    { directory: 'client', command: 'npm', args: ['ci', '--no-audit', '--no-fund'] },
  ]);
});
