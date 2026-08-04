#!/usr/bin/env node
// Stack Bench: run the whole benchmark for one backend, unattended.
//
// For each level: build (or upgrade), grade, and if anything failed hand the
// agent a behavioural bug report and let it fix — up to --fix-rounds times —
// re-grading after each attempt. Records score, cost, time and fix rounds per
// level, then writes a summary.
//
// Usage:
//   node bench.mjs --backend spacetime --levels 1-3 [--track spec]
//                  [--model claude-sonnet-5] [--fix-rounds 3] [--run-index 0]
//                  [--out <dir>]

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(ROOT, '..', '..');
const RUN_SH = join(REPO, 'tools', 'llm-sequential-upgrade', 'run.sh');

const VITE_BASE = { spacetime: 6173, postgres: 6273, mongodb: 6373 };

function parseArgs(argv) {
  const a = { track: 'spec', model: 'claude-sonnet-5', fixRounds: 3, runIndex: 0, levels: '1' };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--backend': a.backend = argv[++i]; break;
      case '--levels': a.levels = argv[++i]; break;
      case '--track': a.track = argv[++i]; break;
      case '--model': a.model = argv[++i]; break;
      case '--fix-rounds': a.fixRounds = parseInt(argv[++i], 10); break;
      case '--run-index': a.runIndex = parseInt(argv[++i], 10); break;
      case '--out': a.out = argv[++i]; break;
      default: console.error(`Unknown argument: ${argv[i]}`); process.exit(2);
    }
  }
  if (!a.backend) {
    console.error('Usage: node bench.mjs --backend <b> --levels 1-3 [--track spec] [--fix-rounds 3]');
    process.exit(2);
  }
  const [from, to] = a.levels.split('-').map(Number);
  a.levelList = Array.from({ length: (to ?? from) - from + 1 }, (_, i) => from + i);
  return a;
}

const sh = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, { encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, ...opts });

function runAgent(args, extra) {
  // run.sh drives the headless coding session; it prints the app dir it used.
  const argv = [RUN_SH, '--track', args.track, '--model', args.model,
    '--run-index', String(args.runIndex), ...extra];
  let out = '';
  try {
    out = sh('bash', argv, { stdio: 'pipe' });
  } catch (err) {
    out = (err.stdout || '').toString();
    if (!/App dir:/.test(out)) throw new Error(`agent session failed: ${out.trim().slice(-400)}`);
  }
  const appDir = (out.match(/App dir:\s+(.+)/) || [])[1]?.trim().replace(/\\/g, '/');
  const cost = Number((out.match(/Total cost: \$([0-9.]+)/) || [])[1] ?? 0);
  const tokens = Number((out.match(/Total tokens: ([0-9,]+)/) || [])[1]?.replace(/,/g, '') ?? 0);
  return { appDir, cost, tokens, out };
}

function grade(args, appDir, url, label) {
  const argv = [join(ROOT, 'run-suite.mjs'), '--app', appDir, '--url', url,
    '--backend', args.backend, '--label', label, '--track', args.track,
    '--restart-cmd', `bash ${join(ROOT, 'restart-backend.sh')} ${args.backend} ${appDir} ${6001 + args.runIndex}`];
  try { sh('node', argv, { stdio: 'inherit' }); } catch { /* score is in the bundle */ }
  const bundle = join(appDir, 'stack-bench', 'bundle.json');
  return existsSync(bundle) ? JSON.parse(readFileSync(bundle, 'utf8')) : null;
}

async function main() {
  const args = parseArgs(process.argv);
  const url = `http://localhost:${VITE_BASE[args.backend] + args.runIndex}`;
  args.out ??= join(ROOT, 'results', `${args.backend}-${args.track}`);
  mkdirSync(args.out, { recursive: true });

  const started = Date.now();
  const run = { backend: args.backend, track: args.track, model: args.model, levels: [] };
  let appDir = null;

  for (const level of args.levelList) {
    const t0 = Date.now();
    console.log(`\n================ ${args.backend} — level ${level} ================`);

    const build = appDir
      ? runAgent(args, ['--upgrade', appDir, '--level', String(level)])
      : runAgent(args, ['--level', String(level), '--backend', args.backend]);
    appDir ??= build.appDir;
    if (!appDir) throw new Error('could not determine the app directory from the agent session');

    let bundle = grade(args, appDir, url, `${args.backend}-l${level}`);
    let fixRounds = 0;
    let fixCost = 0;

    // Hand back findings and let the agent fix, until clean or out of rounds.
    while (fixRounds < args.fixRounds) {
      let wroteReport = true;
      try {
        sh('node', [join(ROOT, 'report-bugs.mjs'), '--app', appDir], { stdio: 'pipe' });
      } catch (err) {
        if (err.status === 3) wroteReport = false;      // nothing failed
        else throw err;
      }
      if (!wroteReport) break;

      fixRounds += 1;
      console.log(`--- fix round ${fixRounds}/${args.fixRounds} ---`);
      const fix = runAgent(args, ['--fix', appDir]);
      fixCost += fix.cost;
      bundle = grade(args, appDir, url, `${args.backend}-l${level}-fix${fixRounds}`);
    }

    run.levels.push({
      level,
      score: bundle?.totals?.score ?? 0,
      max: bundle?.totals?.max ?? 0,
      contractPass: bundle?.totals?.contractPass ?? null,
      code: bundle?.code ?? null,
      buildCostUsd: build.cost,
      fixCostUsd: Number(fixCost.toFixed(4)),
      tokens: build.tokens,
      fixRounds,
      durationSec: Math.round((Date.now() - t0) / 1000),
    });
    writeFileSync(join(args.out, 'run.json'), JSON.stringify(run, null, 2));
  }

  run.totals = {
    score: run.levels.reduce((n, l) => n + l.score, 0),
    max: run.levels.reduce((n, l) => n + l.max, 0),
    costUsd: Number(run.levels.reduce((n, l) => n + l.buildCostUsd + l.fixCostUsd, 0).toFixed(4)),
    fixRounds: run.levels.reduce((n, l) => n + l.fixRounds, 0),
    durationSec: Math.round((Date.now() - started) / 1000),
  };
  writeFileSync(join(args.out, 'run.json'), JSON.stringify(run, null, 2));

  console.log(`\n================ ${args.backend} summary ================`);
  for (const l of run.levels) {
    console.log(`  L${l.level}: ${l.score}/${l.max}  ${l.fixRounds} fix round(s)  ` +
      `$${(l.buildCostUsd + l.fixCostUsd).toFixed(2)}  ${l.durationSec}s`);
  }
  console.log(`  TOTAL ${run.totals.score}/${run.totals.max}  ` +
    `$${run.totals.costUsd}  ${run.totals.fixRounds} fix round(s)  ${run.totals.durationSec}s`);
  console.log(`  ${join(args.out, 'run.json')}`);
}

main();
