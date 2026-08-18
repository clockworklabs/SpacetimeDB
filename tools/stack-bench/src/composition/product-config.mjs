export const DEFAULT_BUILD_IMAGE = 'stack-bench-build:2.1.226';

export const PREFLIGHT_RESOURCE_FLOORS = Object.freeze({
  cpuCount: 4,
  memoryBytes: 8 * 1024 ** 3,
  resultDiskBytes: 10 * 1024 ** 3,
  clockSkewMs: 5_000,
});

export const BUILD_OUTBOUND_DESTINATIONS = Object.freeze([
  'https://registry.npmjs.org',
]);
