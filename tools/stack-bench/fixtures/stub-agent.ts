#!/usr/bin/env node
// Deterministic stand-in for the coding agent in orchestration tests.
//
// Swaps a fixture app into place instead of calling a model: `build` installs
// the broken one, `fix` installs the good one. That makes the loop's branches —
// bug report written, fix session invoked, re-grade, score moves, cap respected
// — deterministic, free and fast to exercise.
import { copyFileSync, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { STACK_BENCH_ROOT } from '../src/package-root.js';

const args: Record<string, string | undefined> = {};
for (let i = 2; i < process.argv.length; i += 2) {
  const option = process.argv[i];
  if (option) args[option.replace(/^--/, '')] = process.argv[i + 1];
}
const app = args.app;
if (!app) throw new Error('stub-agent requires --app');

const resumed = join(app, '..', '.stub-resumed');
if (args.mode === 'resume' && args.model === 'deterministic-deferred') {
  writeFileSync(resumed, 'ready\n');
}
const canFix = args.model !== 'deterministic-stall'
  && (args.model !== 'deterministic-deferred' || existsSync(resumed));
const which = args.mode === 'fix' && canFix
  ? 'app-good' : 'app-broken';
mkdirSync(app, { recursive: true });
copyFileSync(join(STACK_BENCH_ROOT, 'fixtures', which, 'index.html'), join(app, 'index.html'));

console.log(JSON.stringify({
  appDir: app, mode: args.mode, level: Number(args.level ?? 1),
  track: args.track, backend: args.backend, model: args.model,
  guidance: args.guidance,
  setup: { isolation: { mode: 'deterministic-fixture' }, session: 'model-free-test' },
  costUsd: args.mode === 'fix' ? 0.05 : args.mode === 'resume' ? 0.1 : 0.5,
  tokens: 1000, outputTokens: 100,
  usage: { input: 100, output: 100, cacheWrite: 300, cacheRead: 500 },
  turns: args.mode === 'fix' ? 2 : args.mode === 'resume' ? 1 : 3,
  promptBytes: args.mode === 'fix' ? 200 : args.mode === 'resume' ? 150 : 300,
  durationMs: 50, sessionId: `stub-${args.mode}`, ok: true,
}));
