// The runner pool and per-stack port slots use the same finite index range.
export const MAX_RUNNER_CAPACITY = 21;

export const DEFAULT_BUILD_IMAGE = 'stack-bench-build:2.1.226';

export function runnerCapacity(env: NodeJS.ProcessEnv = process.env): number {
  const value = env.STACK_BENCH_RUNNER_CAPACITY ?? '1';
  if (!/^[1-9]\d*$/.test(value) || !Number.isSafeInteger(Number(value))) {
    throw new Error('STACK_BENCH_RUNNER_CAPACITY must be a positive integer');
  }
  if (Number(value) > MAX_RUNNER_CAPACITY) {
    throw new Error(`STACK_BENCH_RUNNER_CAPACITY must be at most ${MAX_RUNNER_CAPACITY}`);
  }
  return Number(value);
}

// Conservative startup policy, not measured requirements for every workload.
export const PREFLIGHT_RESOURCE_FLOORS = Object.freeze({
  cpuCount: 4,
  memoryBytes: 8 * 1024 ** 3,
  resultDiskBytes: 10 * 1024 ** 3,
  clockSkewMs: 5_000,
});

// Every build container runs with the same enforced Docker limits. These are
// caps, not reserved CPU or RAM, and do not define a measured host minimum.
export const BUILD_CONTAINER_RESOURCE_LIMITS = Object.freeze({
  cpuCount: 2,
  memoryBytes: 4 * 1024 ** 3,
  memorySwapBytes: 4 * 1024 ** 3,
  pids: 512,
});

export const SIDECAR_CONTAINER_RESOURCE_LIMITS = Object.freeze({ cpuCount: 1, memoryBytes: 1024 ** 3, pids: 256 });
export const BROKER_CONTAINER_RESOURCE_LIMITS = Object.freeze({ memoryBytes: 256 * 1024 ** 2, pids: 32 });

// Planning totals for one worker. Broker CPU and shared services are not capped
// here; the controller, package cache, and Docker also need resources.
export const ATTEMPT_CONTAINER_LIMIT_TOTALS = Object.freeze({
  cpuCount: BUILD_CONTAINER_RESOURCE_LIMITS.cpuCount + 2 * SIDECAR_CONTAINER_RESOURCE_LIMITS.cpuCount,
  memoryBytes: BUILD_CONTAINER_RESOURCE_LIMITS.memoryBytes + 2 * SIDECAR_CONTAINER_RESOURCE_LIMITS.memoryBytes
    + BROKER_CONTAINER_RESOURCE_LIMITS.memoryBytes,
});

export const BUILD_OUTBOUND_DESTINATIONS = Object.freeze([
  'https://registry.npmjs.org',
]);
