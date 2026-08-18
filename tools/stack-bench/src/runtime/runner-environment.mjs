import { execFileSync } from 'node:child_process';

export const RUNNER_OBSERVATION_FIELDS = Object.freeze([
  'dockerEngineVersion',
  'dockerOs',
  'dockerArchitecture',
  'kernelVersion',
  'cpuCount',
  'memoryBytes',
]);

export function missingRunnerObservation(runner) {
  return RUNNER_OBSERVATION_FIELDS.filter(field => runner?.[field] === undefined);
}

function inspectDocker() {
  try {
    return JSON.parse(execFileSync('docker', ['info', '--format', '{{json .}}'], {
      encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'],
    }));
  } catch (error) {
    const detail = error.stderr?.toString().trim() || error.message;
    throw new Error(`could not inspect the appliance Docker daemon: ${detail}`);
  }
}

function stringField(value, source) {
  if (typeof value !== 'string' || !value) {
    throw new Error(`Docker daemon inspection did not return ${source}`);
  }
  return value;
}

function positiveInteger(value, source) {
  if (!Number.isSafeInteger(value) || value < 1) {
    throw new Error(`Docker daemon inspection did not return a positive integer ${source}`);
  }
  return value;
}

export function controllerRunner({ env = process.env, platform = process.platform,
  architecture = process.arch, dockerInfo } = {}) {
  const mode = env.STACK_BENCH_APPLIANCE === '1' ? 'appliance' : 'local-controller';
  const runner = { schemaVersion: 1, mode, platform, architecture };
  if (mode !== 'appliance') return runner;

  const info = dockerInfo ?? inspectDocker();
  return {
    ...runner,
    dockerEngineVersion: stringField(info.ServerVersion, 'ServerVersion'),
    dockerOs: stringField(info.OSType, 'OSType'),
    dockerArchitecture: stringField(info.Architecture, 'Architecture'),
    kernelVersion: stringField(info.KernelVersion, 'KernelVersion'),
    cpuCount: positiveInteger(info.NCPU, 'NCPU'),
    memoryBytes: positiveInteger(info.MemTotal, 'MemTotal'),
  };
}
