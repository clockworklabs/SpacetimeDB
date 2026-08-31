import assert from 'node:assert/strict';
import test from 'node:test';

import { controllerRunner, missingRunnerObservation, RUNNER_OBSERVATION_FIELDS }
  from '../src/runtime/runner-environment.js';

test('local controllers record only host identity', () => {
  assert.deepEqual(controllerRunner({ env: {}, platform: 'win32', architecture: 'x64' }), {
    schemaVersion: 1,
    mode: 'local-controller',
    platform: 'win32',
    architecture: 'x64',
  });
});

test('appliance controllers record Docker daemon observations', () => {
  const runner = controllerRunner({
    env: { STACK_BENCH_APPLIANCE: '1' },
    platform: 'linux',
    architecture: 'x64',
    dockerInfo: {
      ServerVersion: '29.1.2',
      OSType: 'linux',
      Architecture: 'x86_64',
      KernelVersion: '6.8.0-test',
      NCPU: 8,
      MemTotal: 16_000_000_000,
    },
  });
  assert.deepEqual(runner, {
    schemaVersion: 1,
    mode: 'appliance',
    platform: 'linux',
    architecture: 'x64',
    dockerEngineVersion: '29.1.2',
    dockerOs: 'linux',
    dockerArchitecture: 'x86_64',
    kernelVersion: '6.8.0-test',
    cpuCount: 8,
    memoryBytes: 16_000_000_000,
  });
  assert.deepEqual(missingRunnerObservation(runner), []);
  assert.deepEqual(missingRunnerObservation(null), RUNNER_OBSERVATION_FIELDS);
});

test('appliance controllers reject incomplete Docker observations', () => {
  assert.throws(() => controllerRunner({
    env: { STACK_BENCH_APPLIANCE: '1' },
    dockerInfo: {},
  }), /Docker daemon inspection did not return ServerVersion/);
});
