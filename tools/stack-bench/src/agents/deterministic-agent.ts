#!/usr/bin/env node
// Model-free coding agent for diagnostics and repair-loop tests.
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const passingApp = '<!doctype html><html><body><h1 id="app-title">Fixture Chat</h1></body></html>\n';
const failingApp = '<!doctype html><html><body><h1>Fixture</h1></body></html>\n';
// Two checks only: account creation and catalog values. Each upgrade breaks
// them again so the CLI integration test must repair an earlier feature.
const ecommerceApp = `<!doctype html><html><body>
<input id="username" data-role="signup-username"><input data-role="signup-password" type="password">
<button data-role="signup-submit" onclick="document.getElementById('user').textContent=document.getElementById('username').value">Sign up</button>
<span id="user" data-role="current-user"></span>
<div data-role="item-card">Air Purifier <span data-role="item-price">189</span><span data-role="item-stock">100</span></div>
</body></html>\n`;

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
writeFileSync(join(app, 'index.html'), args.mode === 'fix' && canFix
  ? args.model === 'deterministic-ecommerce' ? ecommerceApp : passingApp : failingApp);

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
