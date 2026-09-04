#!/usr/bin/env node
// Turns grading results into a behavioral BUG_REPORT.md for the fix agent.
//
// Every line the agent reads comes from one of three sources: the sentence
// the agent was already given for the behavior (`statedBy`, else the
// criterion's description), the rendered finding from the catalog, or the
// application's own console errors. Harness prose never enters the report.

import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs as parseNodeArgs } from 'node:util';
import { renderFinding } from '../src/actions/action-findings.js';
import type { Finding } from '../src/actions/action-findings.js';
import { sanitiseConsoleError, sanitiseDiagnostic } from '../src/evidence/diagnostic-sanitizer.js';
import { ARTIFACT_FILE, readArtifactPayload } from '../src/evidence/artifacts.js';
import { criterionEvidence, evidenceIsRepairable } from '../src/evidence/check-evidence.js';
import { assertAgentVisibleText } from '../src/composition/agent-visible-contract.js';
import { CODING_CONTAINER_BUG_REPORT_FILE, CODING_CONTAINER_START_SCRIPT }
  from '../src/runtime/coding-container-policy.js';

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
  results: string;
  out: string;
  archive?: string;
  history: RepairHistoryEntry[];
  checks: string[] | null;
  controls: string[] | null;
  priorRegression: string | null;
  regressionContext: boolean;
}

interface ParsedArgs {
  app?: string;
  results?: string;
  out?: string;
  archive?: string;
  history?: unknown;
  checks?: unknown;
  controls?: unknown;
}

interface Criterion {
  id?: string;
  stableKey?: string;
  desc?: string;
  statedBy?: string;
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
  actor: string | null;
  action: string | null;
  expected: string;
  observed: string;
  consoleErrors: string[];
  contract: boolean;
}

// The public verb for the step that failed. Control and action names are the
// agent's own vocabulary; nothing else about the step is repeated.
function failedAction(action: string | undefined, finding: Finding | null): string | null {
  if (action === 'fill') {
    return finding?.kind === 'choice-missing' ? 'Select the requested choice' : 'Enter the requested value';
  }
  if (action === 'click') return 'Use the requested control';
  if (action === 'signIn') return 'Sign in';
  if (action === 'signUp') return 'Create the account';
  if (action === 'reload') return 'Reload the page';
  return null;
}

// What the application did, from the finding alone. A failure without a
// finding (the feature's setup failed before this behavior was reached)
// says so and nothing more.
function observed(finding: Finding | null, phase: string): string {
  if (finding) return renderFinding(finding);
  return phase === 'setup'
    ? 'the application did not reach this behavior; an earlier step of the same feature failed'
    : 'the application did not do this';
}

export function parseReportBugsArgs(argv: string[]): ReportBugsArgs {
  const { values } = parseNodeArgs({ args: argv.slice(2), options: {
    app: { type: 'string' }, results: { type: 'string' }, out: { type: 'string' },
    archive: { type: 'string' },
    'history-json': { type: 'string' }, 'checks-json': { type: 'string' },
    'controls-json': { type: 'string' },
    'prior-regression': { type: 'string' },
    'regression-context': { type: 'boolean' },
  } });
  const args: ParsedArgs = { app: values.app, results: values.results, out: values.out,
    archive: values.archive,
    history: values['history-json'] === undefined ? undefined : JSON.parse(values['history-json']),
    checks: values['checks-json'] === undefined ? undefined : JSON.parse(values['checks-json']),
    controls: values['controls-json'] === undefined ? undefined : JSON.parse(values['controls-json']) };
  if (!args.app) {
    throw new Error('Usage: report-bugs --app <dir> [--out <file>]');
  }
  args.results ??= join(args.app, 'stack-bench');
  args.out ??= join(args.app, CODING_CONTAINER_BUG_REPORT_FILE);
  args.history ??= [];
  if (!Array.isArray(args.history)) throw new Error('--history-json must contain an array');
  args.checks ??= null;
  if (args.checks !== null && (!Array.isArray(args.checks)
    || args.checks.some(check => typeof check !== 'string' || !check)
    || new Set(args.checks).size !== args.checks.length)) {
    throw new Error('--checks-json must contain distinct non-empty strings');
  }
  args.controls ??= null;
  if (args.controls !== null && (!Array.isArray(args.controls)
    || args.controls.some(control => typeof control !== 'string' || !control)
    || new Set(args.controls).size !== args.controls.length)) {
    throw new Error('--controls-json must contain distinct non-empty strings');
  }
  return { app: args.app, results: args.results, out: args.out, archive: args.archive,
    history: args.history as RepairHistoryEntry[], checks: args.checks as string[] | null,
    controls: args.controls as string[] | null,
    priorRegression: values['prior-regression'] ?? null,
    regressionContext: values['regression-context'] ?? false };
}

function priorRegressionSection(path: string): string[] {
  const details = assertAgentVisibleText(readFileSync(resolve(path), 'utf8')).trim()
    .replace(/^### /gm, '#### ')
    .replace(/^## /gm, '### ');
  if (!details) throw new Error('prior regression report has no failure details');
  return [
    '## Previous repair regression',
    '',
    'The previous repair was rolled back because it broke behavior that already worked.',
    'Keep this behavior working while you fix the current problems.',
    '',
    ...details.split(/\r?\n/),
    '',
  ];
}

export function createBugReport(args: ReportBugsArgs): number {
  const resultsDir = resolve(args.results);
  if (!existsSync(resultsDir)) throw new Error(`No grading results in ${resultsDir}`);

  const bugs: RepairBug[] = [];
  const selectedChecks = args.checks === null ? null : new Set(args.checks);
  const selectedControls = args.controls === null ? null : new Set(args.controls);

  for (const file of readdirSync(resultsDir).filter(name => /^grading-.*\.json$/.test(name))) {
    const report = readArtifactPayload<GradePayload>(join(resultsDir, file), { expectedKind: 'grade' });
    for (const feature of report.features ?? []) {
      // Repairs receive only scored, typed application failures.
      for (const criterion of feature.criteria ?? []) {
        if (selectedChecks && (!criterion.stableKey
          || !selectedChecks.has(criterion.stableKey))) continue;
        if (!(Number(criterion.points) > 0)) continue;
        const evidence = criterionEvidence(criterion);
        if (!evidenceIsRepairable(evidence)) continue;
        const actionEntry = evidence.actions.at(-1);
        const actionId = actionEntry && typeof actionEntry.evidence === 'object' && actionEntry.evidence
          ? String((actionEntry.evidence as { action?: { id?: string } }).action?.id ?? '') : undefined;
        const expected = (criterion.statedBy ?? criterion.desc ?? '').trim() || 'the requested behavior';
        bugs.push({
          area: sanitiseDiagnostic(feature.name, 120),
          actor: sanitiseDiagnostic(actionEntry?.actor ?? evidence.actor, 120) || null,
          action: failedAction(actionId, evidence.finding),
          expected,
          observed: observed(evidence.finding, evidence.phase),
          consoleErrors: (feature.consoleErrors ?? []).slice(0, 3)
            .map(sanitiseConsoleError).filter(Boolean),
          contract: false,
        });
      }
    }
  }

  // Contract failures are separate because the interface name is itself the public
  // requirement here. Behavioral failures above must never expose one.
  const lintPath = join(resultsDir, ARTIFACT_FILE.contractLint);
  if (existsSync(lintPath)) {
    const lint = readArtifactPayload<ContractLintPayload>(lintPath, { expectedKind: 'contract_lint' });
    for (const result of (lint.results ?? []).filter(item => item.status === 'FAIL'
      && (!selectedControls || selectedControls.has(item.id)))) {
      bugs.push({
        area: 'Application interface',
        actor: null,
        action: null,
        expected: `A visible element for "${(result.detail ?? '').split('expected: ').pop()}" must use the "${result.id}" application interface`,
        observed: sanitiseDiagnostic(result.detail
          ?? `no visible element with id="${result.id}" was found after a clean reset`, 500),
        consoleErrors: [], contract: true,
      });
    }
  }

  const bundlePath = join(resultsDir, ARTIFACT_FILE.gradeBundle);
  if (existsSync(bundlePath)) {
    const bundle = readArtifactPayload<GradeBundlePayload>(bundlePath, { expectedKind: 'grade_bundle' });
    if (bundle.outcome?.kind === 'app_failure' && bundle.outcome.reason) {
      const expectedByPhase: Record<string, string> = {
        'database-provenance': `The app must use the ${bundle.backend} database and connection supplied for this run.`,
        'application-layout': 'The app must use a project layout that can be built, started, and reset repeatedly.',
        'application-restart': `The app must provide ${CODING_CONTAINER_START_SCRIPT}. From clean source, it must install dependencies, build, and start the complete application without changing source files.`,
      };
      const expected = expectedByPhase[bundle.outcome.phase ?? '']
        ?? 'The app must start successfully in the supplied environment.';
      bugs.unshift({
        area: 'Application setup',
        actor: null,
        action: null,
        expected,
        observed: sanitiseDiagnostic(bundle.outcome.reason, 500),
        consoleErrors: [],
        contract: false,
      });
    }
  }

  if (bugs.length === 0) {
    console.log('No failures — no bug report written.');
    return 3;
  }

  const behavioral = bugs.filter(bug => !bug.contract);
  const contractFailures = bugs.filter(bug => bug.contract);
  const lines = args.regressionContext ? [] : [
    '# Bug Report',
    '',
    'The application has these problems after a clean database reset and a fresh',
    'restart. Fix the behavior, then redeploy.',
    'Do not change behavior that is already correct. A result from existing local',
    'state does not replace the clean result below.',
    '',
  ];

  if (!args.regressionContext && args.history.length) {
    lines.push('## Earlier work', '');
    lines.push('Earlier changes did not fix the current problems. Use the current source as',
      'the starting point. Do not repeat an earlier approach only because it appeared',
      'to work with existing local state.', '');
  }

  if (behavioral.length) {
    lines.push('## Behavior', '');
    behavioral.forEach((bug, index) => {
      lines.push(`### Bug ${index + 1}: ${bug.area}`, '');
      if (bug.actor) lines.push(`**Actor/session:** ${bug.actor}`, '');
      if (bug.action) lines.push(`**Failed action:** ${bug.action}`, '');
      lines.push(`**Expected:** ${bug.expected}`, '');
      lines.push(`**Actual:** ${bug.observed}`, '');
      if (bug.consoleErrors.length) {
        lines.push('**Console or network errors:**', '');
        bug.consoleErrors.forEach(error => lines.push(`- \`${error}\``));
        lines.push('');
      }
    });
  }

  if (contractFailures.length) {
    lines.push('## Application interface', '');
    lines.push('These required elements were not available in the clean application state:', '');
    contractFailures.forEach(bug => {
      lines.push(`- **Expected:** ${bug.expected}`);
      lines.push(`  **Actual:** ${bug.observed}`);
    });
    lines.push('');
  }

  if (args.priorRegression) lines.push(...priorRegressionSection(args.priorRegression));

  const reportText = assertAgentVisibleText(lines.join('\n'));
  writeFileSync(args.out, reportText);
  if (args.archive) {
    mkdirSync(dirname(args.archive), { recursive: true });
    writeFileSync(args.archive, reportText);
  }
  console.log(`Wrote ${bugs.length} bug(s) to ${args.out}`);
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
