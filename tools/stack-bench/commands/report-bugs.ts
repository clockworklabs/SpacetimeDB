#!/usr/bin/env node
// Turns grading results into a behavioural BUG_REPORT.md for the fix agent.
// Selectors, test mechanics, local topology and raw paths are deliberately
// removed so a fix cannot overfit the harness instead of repairing the app.

import { existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { sanitiseConsoleError, sanitiseDiagnostic } from '../src/evidence/diagnostic-sanitizer.mjs';
import { emptyArtifactIdentities, readArtifact, readArtifactPayload, writeArtifact } from '../src/evidence/artifacts.mjs';
import { criterionEvidence, evidenceIsRepairable } from '../src/evidence/check-evidence.mjs';
import { renderRepairDiagnostic } from '../src/evidence/evidence-presentation.mjs';

interface RepairHistoryEntry {
  round?: number;
  beforeScore?: number;
  beforeMax?: number;
  afterScore?: number;
  afterMax?: number;
  result?: string;
  remainingFailures?: string[];
}

interface ReportBugsArgs {
  app: string;
  out: string;
  archive?: string;
  history: RepairHistoryEntry[];
}

interface ParsedArgs {
  app?: string;
  out?: string;
  archive?: string;
  history?: unknown;
}

interface Criterion {
  id?: string;
  desc?: string;
  points?: number;
  evidence?: unknown;
}

interface GradeFeature {
  name?: string;
  consoleErrors?: string[];
  criteria?: Criterion[];
}

interface GradePayload {
  features?: GradeFeature[];
}

interface ContractResult {
  id: string;
  status: string;
  detail?: string;
}

interface ContractLintPayload {
  results?: ContractResult[];
}

interface GradeBundlePayload {
  backend?: string;
  outcome?: { kind?: string; phase?: string; reason?: string };
}

interface RepairBug {
  area: string;
  expected: string;
  observed: string;
  consoleErrors: string[];
  contract: boolean;
  vague: boolean;
}

export function parseReportBugsArgs(argv: string[]): ReportBugsArgs {
  const args: ParsedArgs = {};
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--app') args.app = argv[++i];
    else if (argv[i] === '--out') args.out = argv[++i];
    else if (argv[i] === '--archive') args.archive = argv[++i];
    else if (argv[i] === '--history-json') {
      const value = argv[++i];
      if (value === undefined) throw new Error('--history-json requires a value');
      args.history = JSON.parse(value);
    }
    else throw new Error(`Unknown argument: ${argv[i]}`);
  }
  if (!args.app) {
    throw new Error('Usage: report-bugs --app <dir> [--out <file>]');
  }
  args.out ??= join(args.app, 'BUG_REPORT.md');
  args.history ??= [];
  if (!Array.isArray(args.history)) throw new Error('--history-json must contain an array');
  return { app: args.app, out: args.out, archive: args.archive,
    history: args.history as RepairHistoryEntry[] };
}

const VAGUE = new Set([
  'it did not behave as described',
  'the feature could not be reached at all',
  'the app did not respond in time',
]);
const REPAIR_HISTORY_LIMIT = 5;

function compactRepairHistory(history: RepairHistoryEntry[]): RepairHistoryEntry[] {
  const distinct = new Map<string, RepairHistoryEntry>();
  for (const item of history) {
    const failures = Array.isArray(item?.remainingFailures)
      ? [...new Set(item.remainingFailures)].sort() : [];
    const key = failures.length ? JSON.stringify(failures) : `round:${item?.round}`;
    distinct.delete(key);
    distinct.set(key, item);
  }
  return [...distinct.values()].slice(-REPAIR_HISTORY_LIMIT);
}

export function createBugReport(args: ReportBugsArgs): number {
  const resultsDir = join(args.app, 'stack-bench');
  if (!existsSync(resultsDir)) throw new Error(`No grading results in ${resultsDir}`);

  let vagueBugs = 0;
  const bugs: RepairBug[] = [];

  for (const file of readdirSync(resultsDir).filter(name => /^grading-.*\.json$/.test(name))) {
    const report = readArtifactPayload<GradePayload>(join(resultsDir, file), { expectedKind: 'grade' });
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
        const vague = VAGUE.has(observed);
        if (vague) vagueBugs += 1;
        bugs.push({
          area: sanitiseDiagnostic(feature.name, 120),
          expected: sanitiseDiagnostic(criterion.desc ?? criterion.id, 300),
          observed,
          consoleErrors: (feature.consoleErrors ?? []).slice(0, 3)
            .map(sanitiseConsoleError).filter(Boolean),
          contract: false, vague,
        });
      }
    }
  }

  // Contract failures are separate because the element id is itself the public
  // requirement here. Behavioural failures above must never expose one.
  const lintPath = join(resultsDir, 'contract-lint.json');
  if (existsSync(lintPath)) {
    const lint = readArtifactPayload<ContractLintPayload>(lintPath, { expectedKind: 'contract_lint' });
    for (const result of (lint.results ?? []).filter(item => item.status === 'FAIL')) {
      bugs.push({
        area: 'Application controls',
        expected: `A visible element for "${(result.detail ?? '').split('expected: ').pop()}" must use id="${result.id}"`,
        observed: sanitiseDiagnostic(result.detail
          ?? `no visible element with id="${result.id}" was found after a clean reset`, 500),
        consoleErrors: [], contract: true, vague: false,
      });
    }
  }

  const bundlePath = join(resultsDir, 'bundle.json');
  if (existsSync(bundlePath)) {
    const bundle = readArtifactPayload<GradeBundlePayload>(bundlePath, { expectedKind: 'grade_bundle' });
    if (bundle.outcome?.kind === 'app_failure' && bundle.outcome.reason) {
      const expectedByPhase: Record<string, string> = {
        'database-provenance': `The app must use the ${bundle.backend} database and connection supplied for this run.`,
        'application-layout': 'The app must use a project layout that can be built, started, and reset repeatedly.',
        'application-restart': 'The app must provide a repeatable command that starts its server after a clean database reset.',
      };
      const expected = expectedByPhase[bundle.outcome.phase ?? '']
        ?? 'The app must start successfully in the supplied environment.';
      bugs.unshift({
        area: 'Application setup',
        expected,
        observed: sanitiseDiagnostic(bundle.outcome.reason, 500),
        consoleErrors: [],
        contract: false, vague: false,
      });
    }
  }

  if (bugs.length === 0) {
    console.log('No failures — no bug report written.');
    return 3;
  }

  const repairBugs = bugs.filter(bug => !bug.vague);
  const behavioural = repairBugs.filter(bug => !bug.contract);
  const contractFailures = repairBugs.filter(bug => bug.contract);
  const lines = [
    '# Bug Report',
    '',
    'The application has these problems after a clean database reset and a fresh',
    'restart. Fix the behaviour, then redeploy.',
    'Do not change behaviour that is already correct. A result from existing local',
    'state does not replace the clean result below.',
    '',
  ];

  if (args.history.length) {
    const history = compactRepairHistory(args.history);
    lines.push('## Earlier repair results', '');
    lines.push('The latest result for each recent failure set is shown:', '');
    for (const item of history) {
      const before = `${item.beforeScore}/${item.beforeMax}`;
      const after = `${item.afterScore}/${item.afterMax}`;
      const remaining = Array.isArray(item.remainingFailures) && item.remainingFailures.length
        ? item.remainingFailures.join(', ') : 'not recorded';
      lines.push(`- Round ${item.round}: ${before} to ${after}; ${item.result}; remaining: ${remaining}`);
    }
    if (args.history.length > history.length) {
      lines.push(`- ${args.history.length - history.length} older or duplicate result(s) omitted`);
    }
    lines.push('', 'Use the current source as the starting point. Do not repeat an earlier',
    'approach only because it appeared to work with existing local state.', '');
  }

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

  if (contractFailures.length) {
    lines.push('## Application controls', '');
    lines.push('These required elements were not available in the clean application state:', '');
    contractFailures.forEach(bug => {
      lines.push(`- **Expected:** ${bug.expected}`);
      lines.push(`  **Actual:** ${bug.observed}`);
    });
    lines.push('');
  }

  const vaguePct = Math.round((vagueBugs / bugs.length) * 100);
  try {
    const bundle = existsSync(bundlePath) ? readArtifact(bundlePath, { expectedKind: 'grade_bundle' }) : null;
    const parentId = bundle?.attempt.id ?? null;
    writeArtifact(join(resultsDir, 'bug-report-quality.json'), {
      kind: 'bug_report_quality', id: `${parentId ?? 'bugs'}-bug-report-quality`,
      attempt: { id: `${parentId ?? 'bugs'}-bug-report-quality`, parentId },
      identities: bundle?.identities ?? emptyArtifactIdentities(),
      payload: { bugs: bugs.length, vague: vagueBugs, vaguePct },
    });
  } catch { /* Quality metadata must not block the repair decision. */ }

  console.log(`  diagnostic quality: ${repairBugs.length}/${bugs.length} actionable, ${vagueBugs} vague (${vaguePct}%)`);
  if (repairBugs.length === 0) {
    rmSync(args.out, { force: true });
    console.log('No actionable failures. A paid repair was not started.');
    return 4;
  }

  const reportText = lines.join('\n');
  writeFileSync(args.out, reportText);
  if (args.archive) {
    mkdirSync(dirname(args.archive), { recursive: true });
    writeFileSync(args.archive, reportText);
  }
  console.log(`Wrote ${repairBugs.length} bug(s) to ${args.out}`);
  return 0;
}

function main(): void {
  try {
    process.exitCode = createBugReport(parseReportBugsArgs(process.argv));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 2;
  }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) main();
