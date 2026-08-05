#!/usr/bin/env node
// Tests the orchestration loop without spending a model call.
//
// bench.mjs runs against a stub agent that installs a broken fixture on build
// and a good one on fix, so every branch — bug report written, fix session
// invoked, re-grade, score moves, cap respected, run.json shape — is exercised
// deterministically in seconds.
//
// The grader is validated separately by grader/mutation-test.mjs. This is about
// the machinery around it.
//
// Usage: node test-loop.mjs

import { execFileSync } from 'node:child_process';
import { readFileSync, existsSync, rmSync, mkdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = dirname(fileURLToPath(import.meta.url));
const WORK = join(ROOT, '.loop-test');
const APP = join(WORK, 'app');

let failures = 0;
const check = (name, ok, detail = '') => {
  console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${name}${ok || !detail ? '' : ` — ${detail}`}`);
  if (!ok) failures += 1;
};

function runBench(extra = []) {
  const argv = [join(ROOT, 'bench.mjs'), '--backend', 'stub', '--levels', '1',
    '--agent', join(ROOT, 'fixtures', 'stub-agent.mjs'),
    '--app', APP, '--out', WORK,
    '--url', `file:///${join(APP, 'index.html').replace(/\\/g, '/')}`, ...extra];
  try {
    return execFileSync('node', argv, { encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  } catch (err) {
    return (err.stdout || '').toString() + (err.stderr || '').toString();
  }
}

rmSync(WORK, { recursive: true, force: true });
mkdirSync(APP, { recursive: true });

console.log('\nLoop test — one fix round available');
const out = runBench(['--fix-rounds', '1']);
const runPath = join(WORK, 'run.json');

check('bench.mjs produced run.json', existsSync(runPath));
if (!existsSync(runPath)) { console.log('\ncannot continue without run.json'); process.exit(1); }

const run = JSON.parse(readFileSync(runPath, 'utf8'));
const level = run.levels?.[0];

check('recorded exactly one level', run.levels?.length === 1);
check('a fix round ran', level?.fixRounds === 1, `fixRounds=${level?.fixRounds}`);
check('the bug report was written', existsSync(join(APP, 'BUG_REPORT.md')));
// Behavioural findings must never reveal how they were detected, or a fix can
// target the check instead of the app. Missing-hook findings are exempt: there
// the test id is the requirement.
const report = readFileSync(join(APP, 'BUG_REPORT.md'), 'utf8');
const behaviourSection = report.split('## Missing testing hooks')[0];
check('behavioural findings do not leak selectors or timings',
  !/data-testid|locator|within \d+ms/.test(behaviourSection));
check('missing hooks are reported separately', /## Missing testing hooks/.test(report));
check('build and fix costs are both recorded',
  level?.buildCostUsd > 0 && level?.fixCostUsd > 0,
  `build=${level?.buildCostUsd} fix=${level?.fixCostUsd}`);
check('grading produced a score out of a maximum', Number.isInteger(level?.score) && level?.max > 0,
  `${level?.score}/${level?.max}`);
check('code metrics captured', level?.code && typeof level.code.totalLoc === 'number',
  JSON.stringify(level?.code));
check('totals aggregate the levels', run.totals?.max === level?.max);
check('wall time recorded', run.totals?.durationSec >= 0);
check('the fix improved the contract lint',
  /CONTRACT LINT FAIL[\s\S]*CONTRACT LINT PASS/.test(out) || level?.contractPass === true,
  'expected the broken fixture to fail the lint and the fixed one to pass');

console.log('\nLoop test — zero fix rounds allowed');
rmSync(WORK, { recursive: true, force: true });
mkdirSync(APP, { recursive: true });
runBench(['--fix-rounds', '0']);
const capped = JSON.parse(readFileSync(runPath, 'utf8'));
check('no fix ran when the cap is zero', capped.levels?.[0]?.fixRounds === 0);
check('no bug report was written when no fix is allowed', !existsSync(join(APP, 'BUG_REPORT.md')));

rmSync(WORK, { recursive: true, force: true });
console.log(`\n${failures === 0 ? 'loop OK' : `${failures} check(s) failed`}`);
process.exit(failures === 0 ? 0 : 1);
