import { isAbsolute, resolve } from 'node:path';

export function operationalOutputRoot(moduleRoot, env = process.env) {
  const configured = env.STACK_BENCH_RESULTS_DIR;
  if (configured === undefined || configured === '') return resolve(moduleRoot);
  if (configured !== configured.trim() || !isAbsolute(configured)) {
    throw new Error('STACK_BENCH_RESULTS_DIR must be an absolute path without surrounding whitespace');
  }
  return resolve(configured);
}
