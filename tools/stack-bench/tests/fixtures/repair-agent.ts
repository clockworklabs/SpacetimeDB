#!/usr/bin/env node
// Deterministic coding agent for repair-loop tests.
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const passingApp = '<!doctype html><html><body><h1 id="app-title">Fixture Chat</h1></body></html>\n';
const failingApp = '<!doctype html><html><body><h1>Fixture</h1></body></html>\n';

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
mkdirSync(app, { recursive: true });
writeFileSync(join(app, 'index.html'), args.mode === 'fix' && canFix ? passingApp : failingApp);

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
