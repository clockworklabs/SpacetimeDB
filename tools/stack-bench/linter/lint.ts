#!/usr/bin/env node
// Scenario-stage hooks require scenario setup and are not linted here.

import { chromium } from 'playwright';
import type { Page } from 'playwright';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs as parseNodeArgs } from 'node:util';
import { loadTrack, DEFAULT_TRACK } from '../src/composition/tracks.js';
import { emptyArtifactIdentities, writeArtifact } from '../src/evidence/artifacts.js';
import { stableElementSelector } from '../src/actions/element-selector.js';

const CHECK_TIMEOUT = 5000;

export interface LintHook {
  id: string;
  element: string;
  stage: string;
  check: 'visible' | 'attached';
  note: string;
  revealedBy?: string;
}

export interface LintResult {
  id: string;
  status: 'PASS' | 'FAIL' | 'BLOCKED' | 'SCENARIO';
  detail?: string;
}

export interface LintArgs {
  url?: string;
  track: string;
  level: number;
  json: boolean;
  headed: boolean;
  out?: string;
  label?: string;
  parentAttemptId?: string;
  credentialAliases?: unknown;
  hooks: string[];
}

export interface LintWalkContext {
  page: Page;
  args: LintArgs;
  hooks: LintHook[];
  byStage(stage: string): LintHook[];
  blocked(stage: string): void;
  checkHook(page: Page, hook: LintHook, results: LintResult[]): Promise<boolean>;
  results: LintResult[];
  uniq: string;
  tid(id: string): string;
  CHECK_TIMEOUT: number;
}

function parseArgs(argv: string[]): LintArgs {
  const { values } = parseNodeArgs({ args: argv.slice(2), options: {
    url: { type: 'string' }, track: { type: 'string' }, level: { type: 'string' },
    json: { type: 'boolean' }, out: { type: 'string' }, label: { type: 'string' },
    'parent-attempt-id': { type: 'string' }, 'credential-aliases-json': { type: 'string' },
    hook: { type: 'string', multiple: true }, headed: { type: 'boolean' },
  } });
  const args: LintArgs = { url: values.url, track: values.track ?? DEFAULT_TRACK,
    level: values.level === undefined ? 1 : Number(values.level), json: values.json ?? false,
    headed: values.headed ?? false, out: values.out, label: values.label,
    parentAttemptId: values['parent-attempt-id'],
    credentialAliases: values['credential-aliases-json'] === undefined
      ? undefined : JSON.parse(values['credential-aliases-json']),
    hooks: values.hook ?? [] };
  if (!args.url || !Number.isInteger(args.level) || args.level < 1) {
    console.error('Usage: node dist/linter/lint.js --url <app-url> --level <N> [--json] [--headed]');
    process.exit(2);
  }
  return args;
}

export function selectHooks(hooks: LintHook[], selectedIds: string[] = []): LintHook[] {
  if (!selectedIds.length) return hooks;
  const remaining = new Set(selectedIds);
  const selected = hooks.filter(hook => remaining.delete(hook.id));
  const unknown: LintHook[] = [...remaining].sort().map(id => ({
    id,
    element: `the selected application control ${id}`,
    stage: 'scenario',
    check: 'visible',
    note: 'checked by the selected feature suite',
  }));
  return [...selected, ...unknown];
}

export function loadHooks(level: number, track: { contracts: string }, selectedIds: string[] = []): LintHook[] {
  const CONTRACTS_DIR = track.contracts;
  const files = readdirSync(CONTRACTS_DIR).filter(f => /^\d+-[a-z-]+\.json$/.test(f)).sort();
  const hooks = [];
  for (const f of files) {
    const contract = JSON.parse(readFileSync(join(CONTRACTS_DIR, f), 'utf8')) as {
      level: number; hooks: LintHook[];
    };
    if (contract.level <= level) hooks.push(...contract.hooks);
  }
  if (hooks.length === 0 && selectedIds.length === 0) {
    console.error(`No contracts found for level ${level} in ${CONTRACTS_DIR}`);
    process.exit(2);
  }
  return selectHooks(hooks, selectedIds);
}

const tid = stableElementSelector;
const uniq = Date.now().toString(36).slice(-5);

async function checkHook(page: Page, hook: LintHook, results: LintResult[]): Promise<boolean> {
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

export function completeUnvisitedHooks(hooks: LintHook[], results: LintResult[]): LintResult[] {
  const visited = new Set(results.map(result => result.id));
  for (const hook of hooks) {
    if (visited.has(hook.id)) continue;
    results.push(hook.stage === 'scenario'
      ? { id: hook.id, status: 'SCENARIO', detail: hook.note }
      : { id: hook.id, status: 'BLOCKED',
          detail: `the core flow did not visit contract stage ${JSON.stringify(hook.stage)}` });
  }
  return results;
}

export function completeAbortedHooks(hooks: LintHook[], results: LintResult[], error: unknown): LintResult[] {
  const visited = new Set(results.map(result => result.id));
  const detail = String(error instanceof Error ? error.message : error ?? 'unknown error')
    .split(/\r?\n/).map(line => line.trim()).filter(Boolean).slice(0, 6).join(' ').slice(0, 800);
  results.push({ id: 'core-flow', status: 'FAIL', detail: `core flow aborted: ${detail}` });
  for (const hook of hooks) {
    if (visited.has(hook.id)) continue;
    if (hook.stage === 'scenario') {
      results.push({ id: hook.id, status: 'SCENARIO', detail: hook.note });
    } else {
      results.push({ id: hook.id, status: 'BLOCKED', detail: 'core flow aborted' });
    }
  }
  return results;
}

async function run() {
  const args = parseArgs(process.argv);
  const track = loadTrack(args.track);
  const hooks = loadHooks(args.level, track, args.hooks);
  const byStage = (stage: string): LintHook[] => hooks.filter(h => h.stage === stage);
  const results: LintResult[] = [];
  const blocked = (stage: string): void => {
    for (const h of hooks.filter(x => x.stage === stage)) {
      results.push({ id: h.id, status: 'BLOCKED', detail: 'earlier core flow step failed' });
    }
  };

  const browser = await chromium.launch({ headless: !args.headed });
  const page = await browser.newContext().then(c => c.newPage());
  page.setDefaultTimeout(CHECK_TIMEOUT);

  try {
    // The core flow is the one part of linting that is entirely
    // application-specific, so each track brings its own.
    const { walk } = await import(pathToFileURL(track.walk).href) as {
      walk(context: LintWalkContext): Promise<void>;
    };
    await walk({ page, args, hooks, byStage, blocked, checkHook, results, uniq, tid, CHECK_TIMEOUT });
    // Every lintable hook must record explicit evidence.
    completeUnvisitedHooks(hooks, results);
  } catch (err: unknown) {
    console.error(`Core flow aborted: ${err instanceof Error ? err.message : String(err)}`);
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
