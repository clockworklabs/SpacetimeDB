#!/usr/bin/env node
// Stack Bench UI-contract linter.
//
// Walks the selected track's declared path using only contract test ids. It
// checks every lintable hook for the requested level. Hooks with stage
// "scenario" need separate setup and are checked by the scenario suites.
//
// Usage: node lint.mjs --url http://localhost:6173 --level 3 [--json] [--headed]
// Exit codes: 0 = all lintable hooks pass, 1 = failures, 2 = usage/infra error.

import { chromium } from 'playwright';
import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { loadTrack, DEFAULT_TRACK } from '../src/composition/tracks.mjs';
import { emptyArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.mjs';
import { stableElementSelector } from '../src/actions/element-selector.mjs';

const ROOT_DIR = join(dirname(fileURLToPath(import.meta.url)), '..');
const CHECK_TIMEOUT = 5000;

function parseArgs(argv) {
  const args = { level: 1, json: false, headed: false, track: DEFAULT_TRACK, hooks: [] };
  for (let i = 2; i < argv.length; i++) {
    switch (argv[i]) {
      case '--url': args.url = argv[++i]; break;
      case '--track': args.track = argv[++i]; break;
      case '--level': args.level = parseInt(argv[++i], 10); break;
      case '--json': args.json = true; break;
      case '--out': args.out = argv[++i]; break;
      case '--label': args.label = argv[++i]; break;
      case '--parent-attempt-id': args.parentAttemptId = argv[++i]; break;
      case '--credential-aliases-json': args.credentialAliases = JSON.parse(argv[++i]); break;
      case '--hook': args.hooks.push(argv[++i]); break;
      case '--headed': args.headed = true; break;
      default:
        console.error(`Unknown argument: ${argv[i]}`);
        process.exit(2);
    }
  }
  if (!args.url || !Number.isInteger(args.level) || args.level < 1) {
    console.error('Usage: node lint.mjs --url <app-url> --level <N> [--json] [--headed]');
    process.exit(2);
  }
  return args;
}

export function selectHooks(hooks, selectedIds = []) {
  if (!selectedIds.length) return hooks;
  const remaining = new Set(selectedIds);
  const selected = hooks.filter(hook => remaining.delete(hook.id));
  const unknown = [...remaining].sort().map(id => ({
    id,
    element: `the selected application control ${id}`,
    stage: 'scenario',
    check: 'visible',
    note: 'checked by the selected feature suite',
  }));
  return [...selected, ...unknown];
}

function loadHooks(level, track, selectedIds = []) {
  const CONTRACTS_DIR = track.contracts;
  const files = readdirSync(CONTRACTS_DIR).filter(f => /^\d+-[a-z-]+\.json$/.test(f)).sort();
  const hooks = [];
  for (const f of files) {
    const contract = JSON.parse(readFileSync(join(CONTRACTS_DIR, f), 'utf8'));
    if (contract.level <= level) hooks.push(...contract.hooks);
  }
  if (hooks.length === 0) {
    console.error(`No contracts found for level ${level} in ${CONTRACTS_DIR}`);
    process.exit(2);
  }
  return selectHooks(hooks, selectedIds);
}

const tid = stableElementSelector;
const uniq = Date.now().toString(36).slice(-5);

async function checkHook(page, hook, results) {
  const loc = page.locator(tid(hook.id)).first();
  try {
    if (hook.revealedBy && !(await loc.count())) {
      await page.locator(tid(hook.revealedBy)).first().click({ timeout: CHECK_TIMEOUT });
    }
    await loc.waitFor({
      state: hook.check === 'visible' ? 'visible' : 'attached',
      timeout: CHECK_TIMEOUT,
    });
    results.push({ id: hook.id, status: 'PASS' });
    return true;
  } catch {
    results.push({
      id: hook.id,
      status: 'FAIL',
      detail: `no element matching ${tid(hook.id)} became ${hook.check} during contract stage ${JSON.stringify(hook.stage)}` +
        (hook.revealedBy ? ` (after clicking ${tid(hook.revealedBy)})` : '') +
        ` — expected: ${hook.element}`,
    });
    return false;
  }
}

export function completeUnvisitedHooks(hooks, results) {
  const visited = new Set(results.map(result => result.id));
  for (const hook of hooks) {
    if (visited.has(hook.id)) continue;
    results.push(hook.stage === 'scenario'
      ? { id: hook.id, status: 'SCENARIO', detail: hook.note }
      : { id: hook.id, status: 'BLOCKED',
          detail: `the golden path did not visit contract stage ${JSON.stringify(hook.stage)}` });
  }
  return results;
}

export function completeAbortedHooks(hooks, results, error) {
  const visited = new Set(results.map(result => result.id));
  const detail = String(error?.message ?? error ?? 'unknown error')
    .split(/\r?\n/).map(line => line.trim()).filter(Boolean).slice(0, 6).join(' ').slice(0, 800);
  results.push({ id: 'golden-path', status: 'FAIL', detail: `golden path aborted: ${detail}` });
  for (const hook of hooks) {
    if (visited.has(hook.id)) continue;
    if (hook.stage === 'scenario') {
      results.push({ id: hook.id, status: 'SCENARIO', detail: hook.note });
    } else {
      results.push({ id: hook.id, status: 'BLOCKED', detail: 'golden path aborted' });
    }
  }
  return results;
}

async function run() {
  const args = parseArgs(process.argv);
  const track = loadTrack(args.track);
  const hooks = loadHooks(args.level, track, args.hooks);
  const byStage = stage => hooks.filter(h => h.stage === stage);
  const results = [];
  const blocked = stage => {
    for (const h of hooks.filter(x => x.stage === stage)) {
      results.push({ id: h.id, status: 'BLOCKED', detail: 'earlier golden-path step failed' });
    }
  };

  const browser = await chromium.launch({ headless: !args.headed });
  const page = await browser.newContext().then(c => c.newPage());
  page.setDefaultTimeout(CHECK_TIMEOUT);

  try {
    // The golden path is the one thing about linting that is entirely
    // application-specific, so each track brings its own.
    const { walk } = await import(pathToFileURL(track.walk).href);
    await walk({ page, args, hooks, byStage, blocked, checkHook, results, uniq, tid, CHECK_TIMEOUT });
    // A successful walk used to omit unknown or forgotten stages and still
    // report PASS. Every lintable hook must now have explicit evidence.
    completeUnvisitedHooks(hooks, results);
  } catch (err) {
    console.error(`Golden path aborted: ${err.message}`);
    completeAbortedHooks(hooks, results, err);
  } finally {
    await browser.close();
  }

  const failures = results.filter(r => r.status === 'FAIL' || r.status === 'BLOCKED');
  const report = {
    label: args.label ?? null,
    url: args.url,
    level: args.level,
    selectedHooks: args.hooks.length ? [...new Set(args.hooks)].sort() : null,
    pass: failures.length === 0,
    counts: {
      lintable: results.filter(r => r.status !== 'SCENARIO').length,
      pass: results.filter(r => r.status === 'PASS').length,
      fail: results.filter(r => r.status === 'FAIL').length,
      blocked: results.filter(r => r.status === 'BLOCKED').length,
      scenario: results.filter(r => r.status === 'SCENARIO').length,
    },
    results,
  };
  if (args.out) {
    const id = `${args.parentAttemptId ?? args.label ?? 'lint'}-contract-lint`;
    writeArtifact(args.out, {
      kind: 'contract_lint', id,
      attempt: { id, parentId: args.parentAttemptId ?? null },
      identities: emptyArtifactIdentities(),
      payload: report,
    });
    if (!args.json) console.log(`\nLint report written to ${args.out}`);
  }
  if (args.json) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    for (const r of results) {
      console.log(`${r.status.padEnd(9)} ${r.id}${r.detail ? ` — ${r.detail}` : ''}`);
    }
    console.log(failures.length === 0
      ? `\nAPPLICATION CONTRACT PASS (${results.filter(r => r.status === 'PASS').length} controls)`
      : `\nAPPLICATION CONTRACT FAIL (${failures.length} control(s) missing or blocked)`);
  }
  process.exit(failures.length === 0 ? 0 : 1);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) run();
