import { performance } from 'node:perf_hooks';

// Evidence needs epoch-shaped timestamps, but Date.now() can move backwards
// when a VM or NTP corrects its wall clock. Anchor the process's monotonic clock
// to its epoch start so elapsed work always produces coherent evidence.
export const evidenceNowMs = () => Math.floor(performance.timeOrigin + performance.now());
