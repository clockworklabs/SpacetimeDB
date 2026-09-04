import { resolve } from 'node:path';
import { parseArgs } from 'node:util';

import { DEFAULT_BUILD_IMAGE } from '../src/composition/product-config.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { stackBenchResultsRoot } from '../src/runtime/operational-paths.js';
import type { PreflightReport, PreflightRequest } from '../src/runtime/preflight.js';

function splitList(value: unknown): string[] {
  return String(value).split(',').map(item => item.trim()).filter(Boolean);
}

export function parsePreflightArgs(
  argv: string[],
  { env = process.env }: { env?: NodeJS.ProcessEnv } = {},
): PreflightRequest {
  const { values } = parseArgs({ args: argv.slice(2), options: {
    backend: { type: 'string', multiple: true },
    track: { type: 'string' },
    levels: { type: 'string' },
    recipe: { type: 'string' },
    'run-index': { type: 'string' },
    parallelism: { type: 'string' },
    'agent-adapter': { type: 'string' },
    guidance: { type: 'string' },
    pack: { type: 'string', multiple: true },
    check: { type: 'string', multiple: true },
    image: { type: 'string' },
    'results-dir': { type: 'string' },
    report: { type: 'string' },
    smoke: { type: 'boolean' },
    json: { type: 'boolean' },
  } });
  const request: PreflightRequest = { backends: [], track: 'ecommerce', levels: '1', levelList: [],
    runIndex: 0, parallelism: 1,
    agentAdapter: 'claude-code', guidance: 'prescribed', packIds: [], checkKeys: [], smoke: false,
    image: env.STACK_BENCH_IMAGE ?? DEFAULT_BUILD_IMAGE,
    resultsDir: stackBenchResultsRoot(STACK_BENCH_ROOT, env) };
  request.backends = (values.backend ?? []).flatMap(splitList);
  if (values.track !== undefined) request.track = values.track;
  if (values.levels !== undefined) request.levels = values.levels;
  if (values.recipe !== undefined) request.recipe = values.recipe;
  if (values['run-index'] !== undefined) request.runIndex = Number(values['run-index']);
  if (values.parallelism !== undefined) request.parallelism = Number(values.parallelism);
  if (values['agent-adapter'] !== undefined) request.agentAdapter = values['agent-adapter'];
  if (values.guidance !== undefined) request.guidance = values.guidance;
  request.packIds = (values.pack ?? []).flatMap(splitList);
  request.checkKeys = (values.check ?? []).flatMap(splitList);
  if (values.image !== undefined) request.image = values.image;
  if (values['results-dir'] !== undefined) request.resultsDir = resolve(values['results-dir']);
  if (values.report !== undefined) request.report = resolve(values.report);
  request.smoke = values.smoke ?? false;
  request.json = values.json;
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
    if (check.remediation && check.status === 'fail') console.log(`        hint: ${check.remediation}`);
  }
  console.log(`\n${report.summary.passed} passed, ${report.summary.failed} failed, ${report.summary.warnings} warnings`);
}
