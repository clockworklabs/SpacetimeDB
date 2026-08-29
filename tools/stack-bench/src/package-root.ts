import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { dirname, parse, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const PACKAGE_NAME = '@spacetimedb/stack-bench';

function isStackBenchPackage(directory: string): boolean {
  const manifestPath = resolve(directory, 'package.json');
  if (!existsSync(manifestPath)) return false;
  try {
    const manifest: unknown = JSON.parse(readFileSync(manifestPath, 'utf8'));
    return typeof manifest === 'object' && manifest !== null
      && 'name' in manifest && manifest.name === PACKAGE_NAME;
  } catch {
    return false;
  }
}

export function findStackBenchRoot(moduleUrl: string | URL = import.meta.url): string {
  let directory = dirname(fileURLToPath(moduleUrl));
  const filesystemRoot = parse(directory).root;
  while (true) {
    if (isStackBenchPackage(directory)) return realpathSync(directory);
    if (directory === filesystemRoot) {
      throw new Error(`cannot find ${PACKAGE_NAME} package root from ${String(moduleUrl)}`);
    }
    directory = dirname(directory);
  }
}

export const STACK_BENCH_ROOT = findStackBenchRoot();
export const REPOSITORY_ROOT = resolve(STACK_BENCH_ROOT, '..', '..');

// An entrypoint that is not TypeScript yet is copied into dist beside the
// compiled modules it imports, so the staged copy is the one that can run.
export function stagedEntrypoint(...segments: readonly string[]): string {
  const staged = resolve(STACK_BENCH_ROOT, 'dist', ...segments);
  return existsSync(staged) ? staged : resolve(STACK_BENCH_ROOT, ...segments);
}
