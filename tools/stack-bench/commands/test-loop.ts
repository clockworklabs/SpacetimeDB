#!/usr/bin/env node
// Tests the orchestration loop without spending a model call.
//
// The benchmark runner uses a stub agent that installs a broken fixture on build
// and a good one on fix, so every branch — bug report written, fix session
// invoked, re-grade, score moves, cap respected, run.json shape — is exercised
// deterministically in seconds.
//
// Mutation tests validate the grader separately. This test covers
// the machinery around it.
//
// Usage: node dist/commands/test-loop.js

import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync, existsSync, rmSync, mkdirSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

import { readArtifact, readArtifactPayload } from '../src/evidence/artifacts.js';
import type { ArtifactIdentities } from '../src/evidence/artifacts.js';
import type { CostRun, CostSession } from '../src/evidence/cost-proof.js';
import type { PublicBackendLease } from '../src/runtime/backend-lease.js';
import type { RepairLevel, RepairOutcome } from '../src/runtime/repair-grant.js';
import type { LevelCheckpoint } from '../src/runtime/source-checkpoint.js';

import { STACK_BENCH_ROOT as ROOT, compiledEntrypoint } from '../src/package-root.js';
const WORK = join(ROOT, '.loop-test');
const APP = join(WORK, 'app');
// A cold Playwright start plus two grades can exceed three minutes on Windows
// Docker hosts. The timeout is a deadlock guard, not a performance assertion.
const BENCH_TIMEOUT_MS = 300_000;

interface LoopSession extends CostSession {
  sessionId?: string;
  tokens?: number;
  turns?: number;
  durationMs?: number;
}

interface LoopLevel extends RepairLevel {
  buildSession?: LoopSession;
  fixSessions?: LoopSession[];
  resumeSession?: LoopSession;
  contractPass?: boolean;
  stalled?: boolean;
  code?: { totalLoc?: number };
  sessionTotals?: { sessions?: number; tokens?: number; turns?: number; durationMs?: number };
}

interface LoopRun extends CostRun {
  id?: string;
  levels?: LoopLevel[];
  outcome?: RepairOutcome;
  artifactEnvelope?: { identities?: ArtifactIdentities };
  backendLease?: PublicBackendLease;
  totals?: CostRun['totals'] & { max?: number; sessions?: number; tokens?: number; turns?: number;
    modelDurationMs?: number; durationSec?: number };
}

interface GradeFeature {
  id?: string;
  setupEvidence?: { schemaVersion?: number; status?: string };
  criteria?: { evidence?: { schemaVersion?: number; status?: string; actions?: unknown[] } }[];
}

interface GradePayload { features?: GradeFeature[]; }
interface SourceCheckpointPayload { source: LevelCheckpoint; }
interface RepairContinuation {
  baseline?: { reproduced?: boolean; score?: number; sourceSha256?: string };
  cumulativeRoundsBefore?: number;
  cumulativeRoundsAfter?: number;
  resumeSetup?: { sourceVerified?: boolean };
}
interface RepairContinuationPayload extends LoopRun { continuation?: RepairContinuation; }

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function processOutput(error: unknown): string {
  if (!isRecord(error)) return String(error);
  return `${String(error.stdout ?? '')}${String(error.stderr ?? '')}`;
}

// A failed assertion or interrupted CI job must not leave a fixture app that a
// later loop can mistake for its own output.
process.on('exit', () => rmSync(WORK, { recursive: true, force: true }));

let failures = 0;
const check = (name: string, ok: boolean, detail = ''): void => {
  console.log(`  ${ok ? 'PASS' : 'FAIL'}  ${name}${ok || !detail ? '' : ` — ${detail}`}`);
  if (!ok) failures += 1;
};

function runBench(extra: string[] = []): string {
  const argv = [compiledEntrypoint('commands', 'bench.js'), '--backend', 'stub', '--levels', '1',
    '--agent-adapter', 'deterministic',
    '--app', APP, '--out', WORK,
    '--track', 'loop',
    '--url', `file:///${join(APP, 'index.html').replace(/\\/g, '/')}`, ...extra];
  try {
    return execFileSync('node', argv, {
      encoding: 'utf8',
      maxBuffer: 32 * 1024 * 1024,
      timeout: BENCH_TIMEOUT_MS,
      killSignal: 'SIGTERM',
    });
  } catch (error: unknown) { return processOutput(error); }
}

const invalidRounds = spawnSync('node', [compiledEntrypoint('commands', 'bench.js'), '--backend', 'stub',
  '--fix-rounds', '1.5'],
  { encoding: 'utf8' });
check('fractional correction budgets are rejected before a run starts',
  invalidRounds.status !== 0
    && /--fix-rounds must be an integer from 0 through 20/.test(invalidRounds.stderr));

rmSync(WORK, { recursive: true, force: true });
mkdirSync(APP, { recursive: true });

console.log('\nLoop test — one fix round available');
const out = runBench(['--fix-rounds', '1']);
const runPath = join(WORK, 'run.json');

check('the benchmark run produced run.json', existsSync(runPath));
if (!existsSync(runPath)) { console.log('\ncannot continue without run.json'); process.exit(1); }

const run = readArtifactPayload<LoopRun>(runPath);
const level = run.levels?.[0];
const evidenceDir = join(APP, 'stack-bench');
const bundleArtifact = readArtifact(join(evidenceDir, 'bundle.json'), { expectedKind: 'grade_bundle' });
const lintArtifact = readArtifact(join(evidenceDir, 'contract-lint.json'), { expectedKind: 'contract_lint' });
const actionArtifact = readArtifact(join(evidenceDir, 'actions.json'), { expectedKind: 'action_check' });
const gradeArtifact = readArtifact<GradePayload>(join(evidenceDir, 'grading-features.json'), { expectedKind: 'grade' });
const leaseArtifact = readArtifact<PublicBackendLease>(join(WORK, 'backend-lease.json'), { expectedKind: 'backend_lease_evidence' });
const checkpointArtifact = readArtifact<SourceCheckpointPayload>(join(WORK, 'level-l1-checkpoint.json'),
  { expectedKind: 'source_checkpoint' });

check('recorded exactly one level', run.levels?.length === 1);
check('run and level carry structured outcomes',
  typeof run.outcome?.kind === 'string' && run.outcome.kind === level?.outcome?.kind,
  `run=${run.outcome?.kind} level=${level?.outcome?.kind}`);
check('artifacts carry the producing run id', typeof run.id === 'string' && run.id.length > 10);
check('run envelope identifies engine, agent adapter, and stack adapter',
  /^[a-f0-9]{64}$/.test(run.artifactEnvelope?.identities?.engine?.sha256 ?? '')
    && /^[a-f0-9]{64}$/.test(run.artifactEnvelope?.identities?.agentAdapter?.sha256 ?? '')
    && run.artifactEnvelope?.identities?.stackAdapter?.id === 'stub');
check('bundle is a child of the run', bundleArtifact.attempt.parentId === run.id,
  JSON.stringify(bundleArtifact.attempt));
check('public lease evidence is a child of the run', leaseArtifact.attempt.parentId === run.id,
  JSON.stringify(leaseArtifact.attempt));
check('level source checkpoint is hash-bound and linked to the run',
  checkpointArtifact.attempt.parentId === run.id
    && level?.checkpoint?.artifact === 'level-l1-checkpoint.json'
    && level.checkpoint.sha256 === checkpointArtifact.payload.source.sha256
    && /^[a-f0-9]{64}$/.test(level.checkpoint.sha256)
    && existsSync(join(WORK, level.checkpoint.directory)),
  JSON.stringify(level?.checkpoint));
check('lint, action, and grade evidence are children of the bundle',
  [lintArtifact, actionArtifact, gradeArtifact]
    .every(artifact => artifact.attempt.parentId === bundleArtifact.attempt.id));
const gradedFeatures = gradeArtifact.payload?.features ?? [];
check('grade artifacts retain typed setup, criterion, and action evidence',
  gradedFeatures.length > 0
    && gradedFeatures.every(feature => feature.setupEvidence?.schemaVersion === 1
      && (feature.criteria ?? []).every(criterion => criterion.evidence?.schemaVersion === 1
        && Array.isArray(criterion.evidence.actions))),
  JSON.stringify(gradedFeatures.map(feature => ({ id: feature.id,
    setup: feature.setupEvidence?.status,
    criteria: feature.criteria?.map(criterion => criterion.evidence?.status) }))));
const publicJson = [runPath, join(WORK, 'backend-lease.json'), join(evidenceDir, 'bundle.json'), join(evidenceDir, 'contract-lint.json'),
  join(evidenceDir, 'actions.json'), join(evidenceDir, 'grading-features.json')]
  .map(path => readFileSync(path, 'utf8')).join('\n');
check('public envelopes contain no secret or lease-token fields',
  !/"(?:apiKey|leaseToken|ownershipToken|password|secret)"\s*:/i.test(publicJson));
check('backend lease was released',
  ['released', 'stopped'].includes(run.backendLease?.state ?? '')
    && (run.backendLease?.resources?.locks?.every(lock => lock.releasedAt) ?? false),
  JSON.stringify(run.backendLease?.state));
check('a fix round ran', level?.fixRounds === 1, `fixRounds=${level?.fixRounds}`);
check('successful repair is explicit', level?.repair?.status === 'corrected'
  && level.repair.budgetRounds === 1 && level.repair.roundsUsed === 1
  && level.repair.stopReason === 'passed',
  JSON.stringify(level?.repair));
const reportPath = join(APP, 'BUG_REPORT.md');
const reportExists = existsSync(reportPath);
check('the bug report was written', reportExists);
// Behavioural findings must never reveal how they were detected, or a fix can
// target the check instead of the app. Missing-control findings are exempt:
// there the element id is the requirement.
const report = reportExists ? readFileSync(reportPath, 'utf8') : '';
const behaviourSection = report.split('## Application controls')[0] ?? '';
check('behavioural findings do not leak selectors or timings',
  !/data-testid|locator|within \d+ms/.test(behaviourSection));
check('missing controls are reported separately', /## Application controls/.test(report));
check('build and fix costs are both recorded',
  (level?.buildCostUsd ?? 0) > 0 && (level?.fixCostUsd ?? 0) > 0,
  `build=${level?.buildCostUsd} fix=${level?.fixCostUsd}`);
check('build and fix sessions remain individually auditable',
  level?.buildSession?.sessionId === 'stub-build'
    && level?.fixSessions?.length === 1
    && level.fixSessions[0]?.sessionId === 'stub-fix',
  JSON.stringify({ build: level?.buildSession, fixes: level?.fixSessions }));
check('level session totals include the build and fix',
  level?.sessionTotals?.sessions === 2
    && level.sessionTotals.tokens === 2000
    && level.sessionTotals.turns === 5
    && level.sessionTotals.durationMs === 100,
  JSON.stringify(level?.sessionTotals));
check('grading produced a score out of a maximum', Number.isInteger(level?.score) && (level?.max ?? 0) > 0,
  `${level?.score}/${level?.max}`);
check('code metrics captured', Boolean(level?.code) && typeof level?.code?.totalLoc === 'number',
  JSON.stringify(level?.code));
check('totals aggregate the levels', run.totals?.max === level?.max);
check('run totals aggregate every model session',
  run.totals?.sessions === 2 && run.totals.tokens === 2000
    && run.totals.turns === 5 && run.totals.modelDurationMs === 100,
  JSON.stringify(run.totals));
check('wall time recorded', (run.totals?.durationSec ?? -1) >= 0);
check('the fix improved the contract lint',
  /APPLICATION CONTRACT FAIL[\s\S]*APPLICATION CONTRACT PASS/.test(out)
    || level?.contractPass === true,
  'expected the broken fixture to fail the lint and the fixed one to pass');

console.log('\nLoop test — zero fix rounds allowed');
rmSync(WORK, { recursive: true, force: true });
mkdirSync(APP, { recursive: true });
runBench(['--fix-rounds', '0']);
const capped = readArtifactPayload<LoopRun>(runPath);
check('no fix ran when the cap is zero', capped.levels?.[0]?.fixRounds === 0);
check('no bug report was written when no fix is allowed', !existsSync(join(APP, 'BUG_REPORT.md')));

console.log('\nLoop test - flat corrections exhaust their declared budget');
rmSync(WORK, { recursive: true, force: true });
mkdirSync(APP, { recursive: true });
runBench(['--fix-rounds', '2', '--model', 'deterministic-stall']);
const exhausted = readArtifactPayload<LoopRun>(runPath);
const exhaustedLevel = exhausted.levels?.[0];
check('both correction rounds ran after the first flat result', exhaustedLevel?.fixRounds === 2,
  `fixRounds=${exhaustedLevel?.fixRounds}`);
check('an unresolved app records budget exhaustion', exhaustedLevel?.repair?.status === 'budget-exhausted'
  && exhaustedLevel.repair.budgetRounds === 2 && exhaustedLevel.repair.roundsUsed === 2
  && exhaustedLevel.repair.stopReason === 'budget-exhausted'
  && exhaustedLevel.stalled === true && exhausted.outcome?.kind === 'app_failure',
  JSON.stringify({ repair: exhaustedLevel?.repair, outcome: exhausted.outcome }));

console.log('\nLoop test - a later finite grant continues the exact exhausted source');
rmSync(WORK, { recursive: true, force: true });
mkdirSync(APP, { recursive: true });
runBench(['--fix-rounds', '2', '--model', 'deterministic-deferred']);
const parentBefore = readFileSync(runPath, 'utf8');
const deferred = readArtifactPayload<LoopRun>(runPath);
check('the deferred parent exhausted its original two-round budget',
  deferred.levels?.[0]?.repair?.status === 'budget-exhausted'
    && deferred.levels[0].repair.roundsUsed === 2,
  JSON.stringify(deferred.levels?.[0]?.repair));
let continuationOutput = '';
try {
  continuationOutput = execFileSync('node', [join(ROOT, 'dist', 'commands', 'repair-cli.js'), 'grant', WORK,
    '--level', '1', '--rounds', '2', '--timeout-minutes', '10'], {
    encoding: 'utf8', maxBuffer: 32 * 1024 * 1024, timeout: BENCH_TIMEOUT_MS,
  });
} catch (error: unknown) {
  continuationOutput = processOutput(error);
}
const continuationRoot = join(WORK, 'continuations');
const continuationDirectories = existsSync(continuationRoot)
  ? readdirSync(continuationRoot, { withFileTypes: true }).filter(entry => entry.isDirectory()) : [];
const continuationDirectory = continuationDirectories.length === 1
  ? join(continuationRoot, continuationDirectories[0]?.name ?? '') : null;
const continuationPath = continuationDirectory ? join(continuationDirectory, 'run.json') : null;
check('repair grant produced one linked continuation',
  continuationPath !== null && existsSync(continuationPath), continuationOutput.slice(-2000));
if (continuationDirectory && continuationPath && existsSync(continuationPath)) {
  const continuationArtifact = readArtifact<RepairContinuationPayload>(continuationPath, { expectedKind: 'repair_continuation' });
  const continuation = continuationArtifact.payload;
  const continuedLevel = continuation.levels?.[0];
  const continuationDetails = continuation.continuation;
  const deferredLevel = deferred.levels?.[0];
  check('continuation reproduced the exact failed baseline before spending a repair round',
    continuationDetails?.baseline?.reproduced === true
      && continuationDetails.baseline.score === deferredLevel?.score
      && continuationDetails.baseline.sourceSha256 === deferredLevel?.checkpoint?.sha256,
    JSON.stringify(continuationDetails?.baseline));
  check('continuation reached correctness inside its finite added budget',
    continuation.outcome?.kind === 'passed'
      && continuedLevel?.repair?.status === 'corrected'
      && continuedLevel.repair.roundsUsed === 1
      && continuationDetails?.cumulativeRoundsBefore === 2
      && continuationDetails?.cumulativeRoundsAfter === 3,
    JSON.stringify({ repair: continuedLevel?.repair, continuation: continuationDetails }));
  check('resume setup is visible, separately costed, and does not consume a repair round',
    continuationDetails?.resumeSetup?.sourceVerified === true
      && continuedLevel?.resumeSession?.sessionId === 'stub-resume'
      && (continuedLevel?.resumeCostUsd ?? 0) > 0
      && continuedLevel.fixRounds === 1,
    JSON.stringify({ setup: continuationDetails?.resumeSetup,
      resume: continuedLevel?.resumeSession, fixes: continuedLevel?.fixRounds }));
  check('continuation process outcome is retained as a typed child artifact',
    readArtifact(join(continuationDirectory, 'process.json'),
      { expectedKind: 'repair_process' }).attempt.parentId === deferred.id);
}
check('grant left the original run artifact byte-for-byte unchanged',
  readFileSync(runPath, 'utf8') === parentBefore);

rmSync(WORK, { recursive: true, force: true });
console.log(`\n${failures === 0 ? 'loop OK' : `${failures} check(s) failed`}`);
process.exit(failures === 0 ? 0 : 1);
