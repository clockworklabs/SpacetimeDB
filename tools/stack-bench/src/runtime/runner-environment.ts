import { execFileSync } from 'node:child_process';

export const STACK_BENCH_RUNNER_PLATFORM = 'linux/amd64' as const;

export const RUNNER_OBSERVATION_FIELDS = Object.freeze([
  'dockerEngineVersion',
  'dockerOs',
  'dockerArchitecture',
  'kernelVersion',
  'cpuCount',
  'memoryBytes',
] as const);

export type RunnerObservationField = typeof RUNNER_OBSERVATION_FIELDS[number];

export interface ControllerRunner {
  schemaVersion: 1;
  mode: 'appliance' | 'local-controller';
  platform: string;
  architecture: string;
  dockerEngineVersion?: string;
  dockerOs?: string;
  dockerArchitecture?: string;
  kernelVersion?: string;
  cpuCount?: number;
  memoryBytes?: number;
}

interface ControllerRunnerOptions {
  env?: NodeJS.ProcessEnv;
  platform?: string;
  architecture?: string;
  dockerInfo?: Record<string, unknown>;
}

export function missingRunnerObservation(
  runner: Partial<Record<RunnerObservationField, unknown>> | null | undefined,
): RunnerObservationField[] {
  return RUNNER_OBSERVATION_FIELDS.filter(field => runner?.[field] === undefined);
}

function errorField(error: unknown, field: 'stderr' | 'message'): unknown {
  return typeof error === 'object' && error !== null ? Reflect.get(error, field) : undefined;
}

function inspectDocker(): Record<string, unknown> {
  try {
    const info: Record<string, unknown> = JSON.parse(
      execFileSync('docker', ['info', '--format', '{{json .}}'], {
        encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], timeout: 120_000,
      }),
    );
    return info;
  } catch (error) {
    const stderr = errorField(error, 'stderr');
    const stderrDetail = stderr === undefined || stderr === null ? '' : String(stderr).trim();
    const detail = stderrDetail || String(errorField(error, 'message') ?? error);
    throw new Error(`could not inspect the appliance Docker daemon: ${detail}`);
  }
}

function stringField(value: unknown, source: string): string {
  if (typeof value !== 'string' || !value) {
    throw new Error(`Docker daemon inspection did not return ${source}`);
  }
  return value;
}

function positiveInteger(value: unknown, source: string): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 1) {
    throw new Error(`Docker daemon inspection did not return a positive integer ${source}`);
  }
  return value;
}

export function controllerRunner({ env = process.env, platform = process.platform,
  architecture = process.arch, dockerInfo }: ControllerRunnerOptions = {}): ControllerRunner {
  const mode = env.STACK_BENCH_APPLIANCE === '1' ? 'appliance' : 'local-controller';
  const runner: ControllerRunner = { schemaVersion: 1, mode, platform, architecture };
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
