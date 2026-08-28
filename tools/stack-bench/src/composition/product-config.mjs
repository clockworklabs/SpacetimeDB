export const DEFAULT_BUILD_IMAGE = 'stack-bench-build:2.1.226';

export const PREFLIGHT_RESOURCE_FLOORS = Object.freeze({
  cpuCount: 4,
  memoryBytes: 8 * 1024 ** 3,
  resultDiskBytes: 10 * 1024 ** 3,
  clockSkewMs: 5_000,
});

// Every build container runs with the same enforced Docker limits. Keep this policy
// in one place so admission, container creation, reuse checks, and evidence
// cannot disagree about the resources assigned to an attempt.
export const BUILD_CONTAINER_RESOURCE_LIMITS = Object.freeze({
  cpuCount: 2,
  memoryBytes: 4 * 1024 ** 3,
  memorySwapBytes: 4 * 1024 ** 3,
  pids: 512,
});

// A concurrent attempt also needs capacity outside its build container for the
// grader, browser, database work, and controller processes.
export const ADDITIONAL_ATTEMPT_RESOURCE_FLOORS = Object.freeze({
  cpuCount: BUILD_CONTAINER_RESOURCE_LIMITS.cpuCount + 1,
  memoryBytes: BUILD_CONTAINER_RESOURCE_LIMITS.memoryBytes + (2 * 1024 ** 3),
});

export function preflightResourceFloors(parallelism = 1) {
  if (!Number.isInteger(parallelism) || parallelism < 1) {
    throw new Error('parallelism must be a positive integer');
  }
  // The single-run floor includes shared services. Each extra attempt reserves
  // its build-container limit plus capacity for work outside that container.
  return Object.freeze({
    ...PREFLIGHT_RESOURCE_FLOORS,
    cpuCount: PREFLIGHT_RESOURCE_FLOORS.cpuCount
      + ((parallelism - 1) * ADDITIONAL_ATTEMPT_RESOURCE_FLOORS.cpuCount),
    memoryBytes: PREFLIGHT_RESOURCE_FLOORS.memoryBytes
      + ((parallelism - 1) * ADDITIONAL_ATTEMPT_RESOURCE_FLOORS.memoryBytes),
  });
}

export const BUILD_OUTBOUND_DESTINATIONS = Object.freeze([
  'https://registry.npmjs.org',
]);
