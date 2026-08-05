#!/usr/bin/env node
// Drives one headless coding session: build a level, upgrade to the next, or fix
// reported bugs. Self-contained — no dependency on the sequential-upgrade tool.
//
// Cost and token usage come from the CLI's own JSON result, so there is no
// telemetry collector to run.
//
// Usage:
//   node agent.mjs --mode build   --backend spacetime --level 1 --app <dir>
//   node agent.mjs --mode upgrade --backend spacetime --level 2 --app <dir>
//   node agent.mjs --mode fix     --backend spacetime --app <dir>
//
// Prints a JSON line: { appDir, costUsd, tokens, durationMs, sessionId, ok }

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, rmSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(ROOT, '..', '..');

const PORTS = {
  spacetime: { vite: 6173 },
  postgres: { vite: 6273, express: 6001, db: 6532 },
  mongodb: { vite: 6373, express: 6001, db: 6537 },
};

function parseArgs(argv) {
  const a = { level: 1, runIndex: 0, model: 'claude-sonnet-5' };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--mode': a.mode = argv[++i]; break;
      case '--backend': a.backend = argv[++i]; break;
      case '--level': a.level = parseInt(argv[++i], 10); break;
      case '--app': a.app = argv[++i]; break;
      case '--run-index': a.runIndex = parseInt(argv[++i], 10); break;
      case '--model': a.model = argv[++i]; break;
      default: console.error(`Unknown argument: ${argv[i]}`); process.exit(2);
    }
  }
  if (!a.mode || !a.backend || !a.app) {
    console.error('Usage: node agent.mjs --mode build|upgrade|fix --backend <b> --app <dir> [--level N]');
    process.exit(2);
  }
  return a;
}

function findClaude() {
  const appData = process.env.APPDATA ?? join(process.env.HOME ?? '', 'AppData', 'Roaming');
  const desktop = join(appData, 'Claude', 'claude-code');
  if (existsSync(desktop)) {
    const versions = readdirSync(desktop).sort();
    const exe = join(desktop, versions[versions.length - 1], 'claude.exe');
    if (existsSync(exe)) return exe;
  }
  return 'claude';
}

const ports = (backend, runIndex) => {
  const p = PORTS[backend];
  return {
    vite: p.vite + runIndex,
    express: p.express ? p.express + runIndex : null,
    dbPort: p.db ?? null,
  };
};

const dbUrl = (backend, runIndex, dbPort) =>
  backend === 'postgres'
    ? `postgresql://stackbench:stackbench@localhost:${dbPort}/stackbench_run${runIndex}`
    : `mongodb://localhost:${dbPort}/stackbench_run${runIndex}`;

// Per-run databases must exist before the app connects, or the agent will go
// looking for one that does — which has led to apps silently using a foreign
// instance.
function ensureDatabase(backend, runIndex, dbPort) {
  const name = `stackbench_run${runIndex}`;
  if (backend === 'postgres') {
    const container = process.env.POSTGRES_CONTAINER ?? 'stack-bench-postgres';
    try {
      execFileSync('docker', ['exec', container, 'psql', '-U', 'stackbench', '-d', 'postgres',
        '-c', `CREATE DATABASE ${name} OWNER stackbench;`], { stdio: 'pipe' });
    } catch { /* already exists */ }
  }
  // Mongo creates databases on first write.
  return name;
}

function moduleName(runIndex) {
  return `stackbench-run${runIndex}`;
}

function backendDoc(args, p) {
  const raw = readFileSync(join(ROOT, 'backends', `${args.backend}.md`), 'utf8');
  return raw
    .replaceAll('<VITE_PORT>', String(p.vite))
    .replaceAll('<EXPRESS_PORT>', String(p.express ?? ''))
    .replaceAll('<MODULE_NAME>', moduleName(args.runIndex))
    .replaceAll('<DATABASE_URL>', p.dbPort ? dbUrl(args.backend, args.runIndex, p.dbPort) : '');
}

// SpacetimeDB is young enough that models have little of it in training data;
// the skill documents are its API reference, equivalent to what the other stacks
// get from having been on the internet for a decade.
function skillDocs(backend) {
  if (backend !== 'spacetime') return '';
  const strip = md => md.replace(/^---\n[\s\S]*?\n---\n/, '');
  return ['typescript-server', 'typescript-client']
    .map(s => strip(readFileSync(join(REPO, 'skills', s, 'SKILL.md'), 'utf8')))
    .join('\n\n---\n\n');
}

function levelPrompt(level) {
  const dir = join(ROOT, 'levels', 'prompts');
  const file = readdirSync(dir).find(f => f.startsWith(String(level).padStart(2, '0') + '-'));
  if (!file) throw new Error(`No prompt for level ${level} in ${dir}`);
  return readFileSync(join(dir, file), 'utf8');
}

function appendix(level) {
  const f = join(ROOT, 'levels', 'contracts', `appendix-${String(level).padStart(2, '0')}.md`);
  return existsSync(f) ? readFileSync(f, 'utf8') : '';
}

function buildPrompt(args, p) {
  const lint = `node "${join(ROOT, 'linter', 'lint.mjs')}" --url http://localhost:${p.vite} --level ${args.level}`;
  const common = [
    `The app lives in ${args.app.replace(/\\/g, '/')} — work there.`,
    '',
    backendDoc(args, p),
  ];
  const skills = skillDocs(args.backend);
  if (skills) common.push('', '---', '', skills);

  if (args.mode === 'fix') {
    return [
      'Fix the bugs reported by automated verification.',
      '',
      'Read BUG_REPORT.md in the app directory. Each entry says what was expected',
      'and what actually happened. Fix the app so the behaviour matches, redeploy,',
      'and make sure the dev server is running.',
      '',
      'Change only what is needed. Do not alter behaviour that is already correct.',
      '',
      `When the fixes are deployed, verify the testing hooks still resolve:\n\n    ${lint}\n`,
      'Output FIX_COMPLETE when done.',
      '',
      ...common,
    ].join('\n');
  }

  const verb = args.mode === 'upgrade'
    ? [
        `Add the level ${args.level} features below to the existing app.`,
        '',
        'Everything from earlier levels already works — do not rewrite it. Add only',
        'what the new level describes, and keep the earlier behaviour intact.',
      ]
    : [`Build the application described below, then deploy it and leave it running.`];

  return [
    ...verb,
    '',
    `When it is deployed, verify the testing hooks resolve:\n\n    ${lint}\n`,
    'Fix any failures and re-run until it prints CONTRACT LINT PASS.',
    `Output ${args.mode === 'upgrade' ? 'UPGRADE_COMPLETE' : 'DEPLOY_COMPLETE'} when the`,
    'dev server is confirmed running and the lint passes.',
    '',
    ...common,
    '',
    '---',
    '',
    levelPrompt(args.level),
    appendix(args.level),
  ].join('\n');
}

function main() {
  const args = parseArgs(process.argv);
  const p = ports(args.backend, args.runIndex);
  if (p.dbPort) ensureDatabase(args.backend, args.runIndex, p.dbPort);
  // `build` means from scratch. Leaving a previous app in place lets the agent
  // inherit code — and a stale BUG_REPORT.md — from a run that has nothing to do
  // with this one.
  if (args.mode === 'build') {
    // A dev server from a previous run can still hold the directory open, so
    // give the filesystem a moment rather than failing the whole build.
    for (let attempt = 0; ; attempt++) {
      try { rmSync(args.app, { recursive: true, force: true }); break; }
      catch (err) {
        if (attempt >= 5) throw err;
        execFileSync(process.platform === 'win32' ? 'timeout' : 'sleep',
          process.platform === 'win32' ? ['/T', '3', '/NOBREAK'] : ['3'], { stdio: 'ignore' });
      }
    }
  }
  mkdirSync(args.app, { recursive: true });
  writeFileSync(join(args.app, '.stack-bench-backend'), args.backend);

  const prompt = buildPrompt(args, p);
  writeFileSync(join(args.app, `.prompt-${args.mode}-l${args.level}.md`), prompt);

  const started = Date.now();
  let raw = '';
  try {
    raw = execFileSync(findClaude(), [
      '--print', '--output-format', 'json',
      '--dangerously-skip-permissions',
      '--model', args.model,
      '--add-dir', args.app,
      '--add-dir', ROOT,
      '-p', prompt,
    ], { cwd: args.app, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });
  } catch (err) {
    raw = (err.stdout || '').toString();
  }

  let result = {};
  try { result = JSON.parse(raw); } catch { /* non-JSON means the session died */ }
  const usage = result.usage ?? {};
  const out = {
    appDir: args.app,
    mode: args.mode,
    level: args.level,
    costUsd: Number((result.total_cost_usd ?? 0).toFixed(4)),
    tokens: (usage.input_tokens ?? 0) + (usage.output_tokens ?? 0)
      + (usage.cache_creation_input_tokens ?? 0) + (usage.cache_read_input_tokens ?? 0),
    outputTokens: usage.output_tokens ?? 0,
    durationMs: Date.now() - started,
    sessionId: result.session_id ?? null,
    ok: result.is_error === false,
  };
  writeFileSync(join(args.app, `.session-${args.mode}-l${args.level}.json`), JSON.stringify({ ...out, text: result.result }, null, 2));
  console.log(JSON.stringify(out));
}

main();
