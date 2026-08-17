#!/usr/bin/env node
// Turns grading results into a behavioural BUG_REPORT.md for the fix agent.
// Selectors, test mechanics, local topology and raw paths are deliberately
// removed so a fix cannot overfit the harness instead of repairing the app.

import { existsSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { sanitiseConsoleError, sanitiseDiagnostic } from './diagnostic-sanitizer.mjs';
import { emptyArtifactIdentities, readArtifact, readArtifactPayload, writeArtifact } from './artifacts.mjs';
import { criterionEvidence, evidenceIsRepairable } from './check-evidence.mjs';
import { renderRepairDiagnostic } from './evidence-presentation.mjs';

function parseArgs(argv) {
  const args = {};
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--app') args.app = argv[++i];
    else if (argv[i] === '--out') args.out = argv[++i];
    else if (argv[i] === '--archive') args.archive = argv[++i];
    else { console.error(`Unknown argument: ${argv[i]}`); process.exit(2); }
  }
  if (!args.app) {
    console.error('Usage: node report-bugs.mjs --app <dir> [--out <file>]');
    process.exit(2);
  }
  args.out ??= join(args.app, 'BUG_REPORT.md');
  return args;
}

const args = parseArgs(process.argv);
const resultsDir = join(args.app, 'stack-bench');
if (!existsSync(resultsDir)) {
  console.error(`No grading results in ${resultsDir}`);
  process.exit(2);
}

const VAGUE = new Set([
  'it did not behave as described',
  'the feature could not be reached at all',
  'the app did not respond in time',
]);
let vagueBugs = 0;
const bugs = [];

for (const file of readdirSync(resultsDir).filter(name => /^grading-.*\.json$/.test(name))) {
  const report = readArtifactPayload(join(resultsDir, file), { expectedKind: 'grade' });
  for (const feature of report.features ?? []) {
    // Only typed application failures are sent to a fix round. Inconclusive or
    // harness-failure evidence describes the benchmark, not the generated app.
    // Zero-point criteria are test-development evidence and never control an
    // ordinary repair loop, even when their behavioral observation failed.
    for (const criterion of feature.criteria ?? []) {
      if (!(Number(criterion.points) > 0)) continue;
      const evidence = criterionEvidence(criterion);
      if (!evidenceIsRepairable(evidence)) continue;
      const observed = renderRepairDiagnostic(evidence);
      if (VAGUE.has(observed)) vagueBugs += 1;
      bugs.push({
        area: sanitiseDiagnostic(feature.name, 120),
        expected: sanitiseDiagnostic(criterion.desc ?? criterion.id, 300),
        observed,
        consoleErrors: (feature.consoleErrors ?? []).slice(0, 3)
          .map(sanitiseConsoleError).filter(Boolean),
        contract: false,
      });
    }
  }
}

// Missing contract hooks are separate because the test id is itself the public
// requirement here. Behavioural failures above must never expose one.
const lintPath = join(resultsDir, 'contract-lint.json');
if (existsSync(lintPath)) {
  const lint = readArtifactPayload(lintPath, { expectedKind: 'contract_lint' });
  for (const result of (lint.results ?? []).filter(item => item.status === 'FAIL')) {
    bugs.push({
      area: 'Testing hooks',
      expected: `The element described as "${(result.detail ?? '').split('expected: ').pop()}" must carry data-testid="${result.id}"`,
      observed: 'no element with that test id was found',
      contract: true,
    });
  }
}

if (bugs.length === 0) {
  console.log('No failures — no bug report written.');
  process.exit(3);
}

const behavioural = bugs.filter(bug => !bug.contract);
const missingHooks = bugs.filter(bug => bug.contract);
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
  behavioural.forEach((bug, index) => {
    lines.push(`### Bug ${index + 1}: ${bug.area}`, '');
    lines.push(`**Expected:** ${bug.expected}`, '');
    lines.push(`**Actual:** ${bug.observed}`, '');
    if (bug.consoleErrors.length) {
      lines.push('**Browser console errors during this feature:**', '');
      bug.consoleErrors.forEach(error => lines.push(`- \`${error}\``));
      lines.push('');
    }
  });
}

if (missingHooks.length) {
  lines.push('## Missing testing hooks', '');
  lines.push('These elements exist in the spec but carry no test id, so they cannot', 'be verified:', '');
  missingHooks.forEach(bug => lines.push(`- ${bug.expected}`));
  lines.push('');
}

writeFileSync(args.out, lines.join('\n'));

const vaguePct = Math.round((vagueBugs / bugs.length) * 100);
console.log(`Wrote ${bugs.length} bug(s) to ${args.out}`);
console.log(`  diagnostic quality: ${bugs.length - vagueBugs}/${bugs.length} actionable, ${vagueBugs} vague (${vaguePct}%)`);
if (vaguePct >= 50) {
  console.log('  !! most of this report says nothing the app can act on.');
  console.log('     A fix round will pay to rediscover what grading already knew.');
}

try {
  const bundlePath = join(resultsDir, 'bundle.json');
  const bundle = existsSync(bundlePath) ? readArtifact(bundlePath, { expectedKind: 'grade_bundle' }) : null;
  const parentId = bundle?.attempt.id ?? null;
  writeArtifact(join(dirname(args.out), 'bug-report-quality.json'), {
    kind: 'bug_report_quality', id: `${parentId ?? 'bugs'}-bug-report-quality`,
    attempt: { id: `${parentId ?? 'bugs'}-bug-report-quality`, parentId },
    identities: bundle?.identities ?? emptyArtifactIdentities(),
    payload: { bugs: bugs.length, vague: vagueBugs, vaguePct },
  });
} catch { /* BUG_REPORT.md is the required artifact; quality metadata is best effort. */ }
