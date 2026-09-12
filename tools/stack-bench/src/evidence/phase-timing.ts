import { performance } from 'node:perf_hooks';

export interface PhaseTiming {
  phase: string;
  suite: string | null;
  durationMs: number;
  threw: boolean;
}

export async function measurePhase<T>(timings: PhaseTiming[], phase: string,
  suite: string | null, work: () => T | Promise<T>): Promise<T> {
  const start = performance.now();
  let threw = true;
  try {
    const result = await work();
    threw = false;
    return result;
  } finally {
    timings.push({ phase, suite, durationMs: performance.now() - start, threw });
  }
}
