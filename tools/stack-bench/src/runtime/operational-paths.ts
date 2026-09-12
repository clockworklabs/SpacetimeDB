import { isAbsolute, resolve } from 'node:path';

export function stackBenchResultsRoot(packageRoot: string,
  env: NodeJS.ProcessEnv = process.env): string {
  const configured = env.STACK_BENCH_RESULTS_DIR;
  if (configured === undefined || configured === '') return resolve(packageRoot, 'results');
  if (configured !== configured.trim() || !isAbsolute(configured)) {
    throw new Error('STACK_BENCH_RESULTS_DIR must be an absolute path without surrounding whitespace');
  }
  return resolve(configured);
}
