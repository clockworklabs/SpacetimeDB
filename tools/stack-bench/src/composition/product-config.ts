export const DEFAULT_BUILD_IMAGE = 'stack-bench-build:2.1.226';

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
// Chromium shares these limits across actors and video encoders. Docker counts
// threads as PIDs; the seven-actor purchase probe exceeds the database's 256 cap.
export const BROWSER_CONTAINER_RESOURCE_LIMITS = Object.freeze({ ...SIDECAR_CONTAINER_RESOURCE_LIMITS,
  memoryBytes: 2 * 1024 ** 3, pids: 512 });
export const BROKER_CONTAINER_RESOURCE_LIMITS = Object.freeze({ memoryBytes: 256 * 1024 ** 2, pids: 32 });

// Planning totals for one worker. Broker CPU and shared services are not capped
// here; the controller, package cache, and Docker also need resources.
export const ATTEMPT_CONTAINER_LIMIT_TOTALS = Object.freeze({
  cpuCount: BUILD_CONTAINER_RESOURCE_LIMITS.cpuCount + SIDECAR_CONTAINER_RESOURCE_LIMITS.cpuCount + BROWSER_CONTAINER_RESOURCE_LIMITS.cpuCount,
  memoryBytes: BUILD_CONTAINER_RESOURCE_LIMITS.memoryBytes + SIDECAR_CONTAINER_RESOURCE_LIMITS.memoryBytes
    + BROWSER_CONTAINER_RESOURCE_LIMITS.memoryBytes
    + BROKER_CONTAINER_RESOURCE_LIMITS.memoryBytes,
});

export const BUILD_OUTBOUND_DESTINATIONS = Object.freeze([
  'https://registry.npmjs.org',
]);

// Actual host ports are leased atomically, including overlaps between indices.
// Browsers and the Fetch standard refuse connections to these ports; an
// application leased one of them can never be reached by the grader.
export const RESTRICTED_PORTS: ReadonlySet<number> = new Set([
  1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102,
  103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465,
  512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993,
  995, 1719, 1720, 1723, 2049, 3659, 4045, 4190, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668,
  6669, 6679, 6697, 10080,
]);
