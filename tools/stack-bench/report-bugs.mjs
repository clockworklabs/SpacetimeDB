#!/usr/bin/env node
// Turns grading results into a BUG_REPORT.md for the fix agent.
//
// Deliberately behavioural: the report says what a user would observe, never how
// the check is implemented. Handing over scenario definitions or selectors would
// let the fix overfit the harness instead of fixing the app.
//
// Usage: node report-bugs.mjs --app <app-dir> [--out <file>]
// Exit: 0 wrote a report, 3 nothing failed (no report written).

import { readFileSync, writeFileSync, existsSync, readdirSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';

function parseArgs(argv) {
  const a = {};
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--app') a.app = argv[++i];
    else if (argv[i] === '--out') a.out = argv[++i];
    else if (argv[i] === '--archive') a.archive = argv[++i];
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
  // Playwright's own vocabulary, which says what a user would have seen.
  if (/intercepts pointer events/.test(detail))
    return 'something invisible was covering the page and absorbing the clicks';
  if (/Page crashed/.test(detail)) return 'the page crashed';
  if (/element is not (visible|enabled)/.test(detail))
    return 'the control was on screen but not usable';

  // Setup failures carry their reason now (grade.mjs). Say which step could not
  // be completed rather than shrugging at the whole feature.
  const setup = detail.match(/setup failed:\s*(.*)$/is);
  if (setup) {
    const why = sanitise(setup[1]);
    if (/current-user|signed-in|session/i.test(setup[1]))
      return 'signing in never completed, so nothing behind it could be reached';
    return why ? `the feature could not be set up: ${why}` : 'the feature could not be reached at all';
  }

  if (/Timeout/.test(detail)) {
    const why = sanitise(detail);
    return why ? `the app did not respond in time: ${why}` : 'the app did not respond in time';
  }

  // Last resort. Pass the observation through with harness internals stripped,
  // rather than replacing it with a phrase that says nothing. Discarding it
  // outright is how a server that ACCEPTED a stranger's write was reported as
  // "it did not behave as described".
  const rest = sanitise(detail);
  return rest ? `it did not behave as described: ${rest}` : 'it did not behave as described';
}

// Strip anything that would let a fix overfit the harness — selectors, test ids,
// file paths, call logs — and keep the observation. The anti-overfitting rule in
// this file's header is the point; throwing the whole message away is not.
function sanitise(detail = '') {
  // The call log is where Playwright says WHY, so mine it before discarding the
  // rest of it. "<div class=backdrop> intercepts pointer events" and "element is
  // not enabled" are the whole answer; "waiting for locator(...)" is noise.
  const raw = String(detail).replace(/\x1B\[[0-9;]*m|\[\d+m/g, ' ');
  const reasons = [...raw.matchAll(/^\s*-\s+(.*)$/gm)]
    .map(m => m[1].trim())
    .filter(l => !/^waiting for|^retrying|^attempting|^scrolling|^done scrolling|^waiting \d|^\d+ ×/i.test(l))
    .filter(l => !/^locator resolved to/i.test(l));
  const why = [...new Set(reasons)].slice(0, 2).join('; ');

  let s = raw
    .replace(/Call log:[\s\S]*/i, why ? ` (${why})` : ' ')
    .replace(/\[data-testid="[^"]*"\]/g, 'the control') // test ids
    .replace(/locator\((?:[^()]|\([^()]*\))*\)/g, 'the control')
    .replace(/\b[A-Za-z]:[\\/][^\s'"]*/g, 'a file')     // windows paths
    .replace(/\bhttps?:\/\/localhost:\d+/g, 'the app')  // local urls
    .replace(/\s+/g, ' ')
    .trim();
  // Nothing left worth saying.
  if (/^(setup failed|timeout[.\s]*)$/i.test(s)) return '';
  return s.length > 220 ? s.slice(0, 217) + '...' : s;
}

const args = parseArgs(process.argv);
const dir = join(args.app, 'stack-bench');
if (!existsSync(dir)) { console.error(`No grading results in ${dir}`); process.exit(2); }

// Phrases that tell the fix agent nothing. Tracked so a run can report how much
// of its own bug report was useless — see the counter below.
const VAGUE = [
  'it did not behave as described',
  'the feature could not be reached at all',
  'the app did not respond in time',
];
let vagueBugs = 0;

const bugs = [];
for (const file of readdirSync(dir).filter(f => /^grading-.*\.json$/.test(f))) {
  const report = JSON.parse(readFileSync(join(dir, file), 'utf8'));
  for (const feature of report.features ?? []) {
    // An INCONCLUSIVE criterion is the harness saying IT could not run the
    // test — not the app saying it is broken. Handing those to a fix round
    // bills real money for chasing defects that do not exist: a SpacetimeDB
    // run spent $6.83 across two rounds on three reported failures, two of
    // which were this (one criterion that passed on the very next grade, and
    // one the grader could never contend). Grading still records and prints
    // them; the bug report is for things the app can actually fix.
    for (const c of feature.criteria.filter(x => !x.passed && !x.inconclusive)) {
      const observed = humanise(c.detail);
      // A report the agent cannot act on is worse than no report: it bills a
      // fix round to rediscover what the grader already knew, and a wrong guess
      // costs a rollback. This was 85% of all failures before the grader
      // started carrying its reasons, and nothing surfaced it — it took an
      // audit script to find. Count it, and say so, so the next regression is
      // visible in the run itself.
      if (VAGUE.some(v => observed === v)) vagueBugs += 1;
      bugs.push({
        area: feature.name,
        expected: c.desc ?? c.id,
        observed,
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

const behavioural = bugs.filter(b => !b.contract);
const missingHooks = bugs.filter(b => b.contract);

const lines = [
  '# Bug Report',
  '',
  'Automated verification found the following problems. Fix the app so the',
  'behaviour matches, then redeploy. Do not change behaviour that is already',
  'correct.',
  '',
];

if (behavioural.length) {
  lines.push('## Behaviour', '');
  behavioural.forEach((b, i) => {
    lines.push(`### Bug ${i + 1}: ${b.area}`, '');
    lines.push(`**Expected:** ${b.expected}`, '');
    lines.push(`**Actual:** ${b.observed}`, '');
    if (b.consoleErrors?.length) {
      lines.push('**Browser console errors during this feature:**', '');
      b.consoleErrors.forEach(e => lines.push(`- \`${e}\``));
      lines.push('');
    }
  });
}

// Kept separate: these name a test id because the test id IS the requirement,
// where a behavioural bug must never mention how it was detected.
if (missingHooks.length) {
  lines.push('## Missing testing hooks', '');
  lines.push('These elements exist in the spec but carry no test id, so they cannot', 'be verified:', '');
  missingHooks.forEach(b => lines.push(`- ${b.expected}`));
  lines.push('');
}

writeFileSync(args.out, lines.join('\n'));

// Say how much of this report is actionable, every time, in the run's own log.
// The grader knows why a check failed; if that did not survive into the report,
// the fix round is being paid to rediscover it. 85% of failures were arriving as
// a bare catch-all and nothing anywhere said so.
const pct = bugs.length ? Math.round((vagueBugs / bugs.length) * 100) : 0;
console.log(`Wrote ${bugs.length} bug(s) to ${args.out}`);
if (bugs.length) {
  console.log(`  diagnostic quality: ${bugs.length - vagueBugs}/${bugs.length} actionable, ${vagueBugs} vague (${pct}%)`);
  if (pct >= 50) {
    console.log('  !! most of this report says nothing the app can act on.');
    console.log('     A fix round will pay to rediscover what grading already knew.');
    console.log('     Check grade.mjs is recording failure reasons, and that');
    console.log('     humanise() in this file has a case for them.');
  }
}
// Machine-readable, so a run can carry it rather than relying on someone reading
// the console.
try {
  writeFileSync(join(dirname(args.out), 'bug-report-quality.json'),
    JSON.stringify({ bugs: bugs.length, vague: vagueBugs, vaguePct: pct }, null, 2));
} catch { /* the report itself is what matters */ }
