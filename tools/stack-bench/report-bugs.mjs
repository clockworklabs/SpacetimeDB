#!/usr/bin/env node
// Turns grading results into a BUG_REPORT.md for the fix agent.
//
// Deliberately behavioural: the report says what a user would observe, never how
// the check is implemented. Handing over scenario definitions or selectors would
// let the fix overfit the harness instead of fixing the app.
//
// Usage: node report-bugs.mjs --app <app-dir> [--out <file>]
// Exit: 0 wrote a report, 3 nothing failed (no report written).

import { readFileSync, writeFileSync, existsSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

function parseArgs(argv) {
  const a = {};
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--app') a.app = argv[++i];
    else if (argv[i] === '--out') a.out = argv[++i];
    else { console.error(`Unknown argument: ${argv[i]}`); process.exit(2); }
  }
  if (!a.app) { console.error('Usage: node report-bugs.mjs --app <dir> [--out <file>]'); process.exit(2); }
  a.out ??= join(a.app, 'BUG_REPORT.md');
  return a;
}

// Failure detail is harness-shaped ("[data-testid=x] not visible within 5000ms").
// Translate to what a person would have seen.
function humanise(detail = '') {
  if (/still visible after/.test(detail)) return 'it was still showing when it should have disappeared';
  if (/not visible within/.test(detail)) return 'it never appeared';
  if (/missing, \d+ duplicated/.test(detail)) {
    const m = detail.match(/(\d+) missing, (\d+) duplicated/);
    const parts = [];
    if (Number(m?.[1])) parts.push(`${m[1]} never arrived`);
    if (Number(m?.[2])) parts.push(`${m[2]} arrived more than once`);
    return parts.join(' and ');
  }
  if (/order differs between/.test(detail)) return 'the two users saw the messages in different orders';
  if (/unexpectedly contains/.test(detail)) return 'it included something it should not have';
  if (/expected exactly (\d+)/.test(detail)) {
    const m = detail.match(/expected exactly (\d+) .*?, found (\d+)/);
    return m ? `there were ${m[2]} of them instead of ${m[1]}` : 'the wrong number of them appeared';
  }
  if (/ACCEPTED a write with a tampered/.test(detail)) return 'the server accepted a request that claimed to be from a different user';
  if (/setup failed/i.test(detail)) return 'the feature could not be reached at all';
  if (/Timeout/.test(detail)) return 'the app did not respond in time';
  return 'it did not behave as described';
}

const args = parseArgs(process.argv);
const dir = join(args.app, 'stack-bench');
if (!existsSync(dir)) { console.error(`No grading results in ${dir}`); process.exit(2); }

const bugs = [];
for (const file of readdirSync(dir).filter(f => /^grading-.*\.json$/.test(f))) {
  const report = JSON.parse(readFileSync(join(dir, file), 'utf8'));
  for (const feature of report.features ?? []) {
    for (const c of feature.criteria.filter(x => !x.passed)) {
      bugs.push({
        area: feature.name,
        expected: c.desc ?? c.id,
        observed: humanise(c.detail),
        consoleErrors: feature.consoleErrors?.slice(0, 3) ?? [],
      });
    }
  }
}

// A contract hook that is missing blocks verification entirely, so report it too.
const lintPath = join(dir, 'contract-lint.json');
if (existsSync(lintPath)) {
  const lint = JSON.parse(readFileSync(lintPath, 'utf8'));
  for (const r of (lint.results ?? []).filter(r => r.status === 'FAIL')) {
    bugs.push({
      area: 'Testing hooks',
      expected: `The element described as "${(r.detail ?? '').split('expected: ').pop()}" must carry data-testid="${r.id}"`,
      observed: 'no element with that test id was found',
      contract: true,
    });
  }
}

if (bugs.length === 0) {
  console.log('No failures — no bug report written.');
  process.exit(3);
}

const lines = [
  '# Bug Report',
  '',
  'Automated verification found the following problems. Each describes what was',
  'expected and what actually happened. Fix the app so the behaviour matches, then',
  'redeploy. Do not change behaviour that is already correct.',
  '',
];
bugs.forEach((b, i) => {
  lines.push(`## Bug ${i + 1}: ${b.area}`, '');
  lines.push(`**Expected:** ${b.expected}`, '');
  lines.push(`**Actual:** ${b.observed}`, '');
  if (b.consoleErrors?.length) {
    lines.push('**Browser console errors during this feature:**', '');
    b.consoleErrors.forEach(e => lines.push(`- \`${e}\``));
    lines.push('');
  }
});

writeFileSync(args.out, lines.join('\n'));
console.log(`Wrote ${bugs.length} bug(s) to ${args.out}`);
