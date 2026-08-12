#!/usr/bin/env node
// Stand-in for agent.mjs when testing the orchestration loop.
//
// Swaps a fixture app into place instead of calling a model: `build` installs
// the broken one, `fix` installs the good one. That makes the loop's branches —
// bug report written, fix session invoked, re-grade, score moves, cap respected
// — deterministic, free and fast to exercise.
import { copyFileSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const args = {};
for (let i = 2; i < process.argv.length; i += 2) args[process.argv[i].replace(/^--/, '')] = process.argv[i + 1];

const which = args.mode === 'fix' ? 'app-good' : 'app-broken';
mkdirSync(args.app, { recursive: true });
copyFileSync(join(HERE, which, 'index.html'), join(args.app, 'index.html'));

console.log(JSON.stringify({
  appDir: args.app, mode: args.mode, level: Number(args.level ?? 1),
  track: args.track, backend: args.backend, model: args.model,
  guidance: args.guidance, stack: args.guidance === 'minimal' ? 'free' : 'prescribed',
  setup: { isolation: { mode: 'deterministic-fixture' }, session: 'model-free-test' },
  costUsd: args.mode === 'fix' ? 0.05 : 0.5, tokens: 1000, outputTokens: 100,
  usage: { input: 100, output: 100, cacheWrite: 300, cacheRead: 500 },
  turns: args.mode === 'fix' ? 2 : 3, promptBytes: args.mode === 'fix' ? 200 : 300,
  durationMs: 50, sessionId: `stub-${args.mode}`, ok: true,
}));
