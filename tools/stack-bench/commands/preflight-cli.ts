import { join, resolve } from 'node:path';

import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import type { PreflightReport, PreflightRequest } from '../src/runtime/preflight.js';

function splitList(value: unknown): string[] {
  return String(value).split(',').map(item => item.trim()).filter(Boolean);
}

function nextArgument(argv: string[], index: number): string {
  const value = argv[index];
  if (value === undefined) throw new Error(`missing value after ${argv[index - 1] ?? 'argument'}`);
  return value;
}

export function parsePreflightArgs(
  argv: string[],
  { env = process.env }: { env?: NodeJS.ProcessEnv } = {},
): PreflightRequest {
  const request: PreflightRequest = { backends: [], track: 'ecommerce', levels: '1', levelList: [],
    runIndex: 0, parallelism: 1,
    agentAdapter: 'claude-code', guidance: 'prescribed', packIds: [], checkKeys: [], smoke: false,
    image: env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE,
    resultsDir: resolve(env.STACK_BENCH_RESULTS_DIR ?? join(STACK_BENCH_ROOT, 'results')) };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--backend': request.backends.push(...splitList(nextArgument(argv, ++i))); break;
      case '--track': request.track = nextArgument(argv, ++i); break;
      case '--levels': request.levels = nextArgument(argv, ++i); break;
      case '--recipe': request.recipe = nextArgument(argv, ++i); break;
      case '--run-index': request.runIndex = Number(nextArgument(argv, ++i)); break;
      case '--parallelism': request.parallelism = Number(nextArgument(argv, ++i)); break;
      case '--agent-adapter': request.agentAdapter = nextArgument(argv, ++i); break;
      case '--guidance': request.guidance = nextArgument(argv, ++i); break;
      case '--pack': request.packIds.push(...splitList(nextArgument(argv, ++i))); break;
      case '--check': request.checkKeys.push(...splitList(nextArgument(argv, ++i))); break;
      case '--image': request.image = nextArgument(argv, ++i); break;
      case '--results-dir': request.resultsDir = resolve(nextArgument(argv, ++i)); break;
      case '--report': request.report = resolve(nextArgument(argv, ++i)); break;
      case '--smoke': request.smoke = true; break;
      case '--json': request.json = true; break;
      default: throw new Error(`unknown argument: ${argv[i]}`);
    }
  }
  if (!request.backends.length) throw new Error('--backend is required (comma-separated values are accepted)');
  if (request.guidance !== 'neutral' && request.guidance !== 'prescribed') {
    throw new Error('--guidance must be neutral or prescribed');
  }
  request.backends = [...new Set(request.backends)].sort();
  if (!Number.isInteger(request.runIndex) || request.runIndex < 0) {
    throw new Error('--run-index must be a non-negative integer');
  }
  if (!Number.isInteger(request.parallelism) || (request.parallelism ?? 0) < 1) {
    throw new Error('--parallelism must be a positive integer');
  }
  const match = String(request.levels).match(/^(\d+)(?:-(\d+))?$/);
  if (!match || Number(match[2] ?? match[1]) < Number(match[1])) {
    throw new Error('--levels must be N or N-M');
  }
  request.levelList = Array.from({ length: Number(match[2] ?? match[1]) - Number(match[1]) + 1 },
    (_, index) => Number(match[1]) + index);
  if (request.recipe && request.levelList.length !== 1) {
    throw new Error('--recipe requires exactly one requested level');
  }
  return request;
}

export function printPreflightReport(report: PreflightReport): void {
  console.log(`Stack Bench preflight: ${report.ok ? 'READY' : 'NOT READY'}`);
  for (const check of report.checks) {
    const mark = check.status === 'pass' ? 'PASS' : check.status === 'warn' ? 'WARN' : 'FAIL';
    console.log(`  ${mark.padEnd(4)}  ${check.id.padEnd(28)} ${check.summary}`);
    if (check.remediation && check.status === 'fail') console.log(`        fix: ${check.remediation}`);
  }
  console.log(`\n${report.summary.passed} passed, ${report.summary.failed} failed, ${report.summary.warnings} warnings`);
}
