import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SOURCE_ROOT = dirname(fileURLToPath(import.meta.url));

export const STACK_BENCH_ROOT = resolve(SOURCE_ROOT, '..');
export const REPOSITORY_ROOT = resolve(STACK_BENCH_ROOT, '..', '..');
