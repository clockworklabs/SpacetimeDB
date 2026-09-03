import { closeSync, existsSync, fstatSync, openSync, readSync, readdirSync, statSync }
  from 'node:fs';
import { basename, join, resolve } from 'node:path';

import type { CampaignAttemptState } from '../src/campaigns/campaign-scheduler.js';
import type { DependencyPromptSelection, DependencyState }
  from '../src/progression/dependency-mode.js';
import type { ProgressionState } from '../src/progression/progression-state.js';
import type { CompiledCampaignPlan } from '../src/campaigns/campaign-compiler.js';
import type { DependencyProgress } from '../src/campaigns/campaign-inspection.js';
import type { GradeBundlePayload } from '../src/evidence/benchmark-run.js';
import { ARTIFACT_FILE, readArtifactPayload } from '../src/evidence/artifacts.js';
import { CAMPAIGN_FILE } from '../src/campaigns/campaign-path.js';
import { campaignFacts, inspectCampaignAttempt } from '../src/campaigns/campaign-inspection.js';
import { campaignLockIsActive } from '../src/campaigns/campaign-lock.js';
import { campaignProgressionOwner } from '../src/campaigns/campaign-compiler.js';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../src/progression/progression-definition.js';
import { progressionEngine } from '../src/progression/progression-engine.js';
import { readCampaignState } from '../src/campaigns/campaign-scheduler.js';
import { readProgressionState } from '../src/progression/progression-state.js';
import { redactCredentials } from '../src/evidence/diagnostic-sanitizer.js';
import { repairBudgetLimit } from '../src/progression/repair-plan.js';
import { MAX_LOG_BYTES, contained, parseRunProgress, readTextTail,
  walkPublicExecutionArtifacts } from './dashboard-model.js';
import type { DashboardArtifact } from './dashboard-model.js';
import { attemptExcluded, attemptMetrics, attemptStalling, compareCampaign, median }
  from './public/metrics.js';

const CAMPAIGN_KEY = /^[a-z0-9][a-z0-9.-]*$/;
const GRADE_DIRECTORY = /^(?:first-build-l(\d+)-grading|l(\d+)-fix(\d+)-grading|grading)$/i;
const PROGRESSION_ATTEMPT = /^attempt-(\d+)$/i;
const LOG_FILE = 'process.stdout.log';

type ControllerActive = (directory: string, campaign: CompiledCampaignPlan) => boolean;
type InspectedAttempt = ReturnType<typeof inspectCampaignAttempt>;

interface ViewOptions {
  controllerActive?: ControllerActive;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function percentage(value: number | null | undefined): number | null {
  return value == null ? null : Math.round(value * 1000) / 10;
}

function campaignDirectory(resultsRoot: string, key: string): string {
  if (!CAMPAIGN_KEY.test(key)) throw new Error('campaign key is invalid');
  return contained(join(resolve(resultsRoot), 'campaigns'), key, 'campaign');
}

function fileFingerprint(path: string): string | null {
  if (!existsSync(path)) return null;
  const stat = statSync(path);
  return `${stat.size}:${stat.mtimeMs}`;
}

// Every file whose change can move a number in the view, and nothing else: a
// running campaign that has written nothing since the last read is unchanged.
function executionFingerprints(directory: string, files: readonly string[]): string[] {
  const attemptsRoot = join(directory, 'attempts');
  if (!existsSync(attemptsRoot)) return [];
  const parts: string[] = [];
  for (const attempt of readdirSync(attemptsRoot, { withFileTypes: true })) {
    if (!attempt.isDirectory()) continue;
    const attemptDirectory = join(attemptsRoot, attempt.name);
    for (const execution of readdirSync(attemptDirectory, { withFileTypes: true })) {
      if (!execution.isDirectory()) continue;
      for (const file of files) {
        const stamp = fileFingerprint(join(attemptDirectory, execution.name, file));
        if (stamp) parts.push(`${attempt.name}/${execution.name}/${file}:${stamp}`);
      }
    }
  }
  return parts.sort();
}

function campaignFingerprint(directory: string, files: readonly string[],
  executionFiles: readonly string[]): string {
  return [...files.map(file => `${file}:${fileFingerprint(join(directory, file)) ?? 'missing'}`),
    ...executionFingerprints(directory, executionFiles)].join('|');
}

// Overview

export interface OverviewCampaign {
  key: string;
  id: string;
  title: string;
  status: string;
  mode: string;
  levels: number[];
  repetitions: number;
  provisional: boolean;
  updatedAt: string | null;
  // The mode's official score per stack, as a percentage; null until a stack
  // has a comparable result.
  scores: Record<string, number | null>;
  attempts: { total: number; running: number; completed: number };
}

export interface UnreadableOverviewCampaign {
  key: string;
  id: string;
  title: string;
  status: 'unreadable';
  error: string;
}

export type OverviewEntry = OverviewCampaign | UnreadableOverviewCampaign;

const overviewCache = new Map<string, {
  fingerprint: string;
  plan: CompiledCampaignPlan;
  campaign: OverviewCampaign;
}>();

function overviewCampaign(directory: string): {
  plan: CompiledCampaignPlan;
  campaign: OverviewCampaign;
} {
  const { plan, state } = readCampaignState(directory, { requireCurrentInputs: false });
  const attempts = state.attempts.map(attempt =>
    inspectCampaignAttempt(plan, attempt, directory));
  const comparison = compareCampaign<InspectedAttempt>({ attempts });
  const scores = Object.fromEntries(plan.stacks.map(stack =>
    [stack.id, percentage(comparison.rows.find(row => row.stack === stack.id)?.final ?? null)]));
  return {
    plan,
    campaign: {
      key: basename(resolve(directory)),
      id: plan.id,
      title: plan.title,
      status: state.status,
      mode: plan.definition.mode?.id ?? 'sequential',
      levels: plan.definition.levels,
      repetitions: plan.definition.repetitions,
      provisional: campaignFacts(plan).grading.status !== 'qualified',
      updatedAt: state.updatedAt,
      scores,
      attempts: { total: state.summary.total, running: state.summary.running,
        completed: state.summary.completed },
    },
  };
}

// Liveness is one more fact about a running campaign, not a condition of
// reading it: the read-only host view has no Docker socket to ask.
function controllerInterrupted(probe: ControllerActive, directory: string,
  plan: CompiledCampaignPlan, status: string): boolean {
  if (status !== 'running') return false;
  try { return !probe(directory, plan); } catch { return false; }
}

function withInterruption(campaign: OverviewCampaign, interrupted: boolean): OverviewCampaign {
  if (!interrupted) return campaign;
  return { ...campaign, status: 'attention-required',
    attempts: { ...campaign.attempts, running: 0 } };
}

// Summaries only: no attempt list, no log, no plan. The fingerprint covers a
// running campaign too, so a poll that finds nothing changed costs one stat
// per evidence file instead of a full replay.
export function overviewSummary(campaignsRoot: string,
  { controllerActive = campaignLockIsActive }: ViewOptions = {}): OverviewEntry[] {
  if (!existsSync(campaignsRoot)) return [];
  const campaigns: OverviewEntry[] = [];
  for (const entry of readdirSync(campaignsRoot, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const directory = join(campaignsRoot, entry.name);
    if (!existsSync(join(directory, CAMPAIGN_FILE.state))
      || !existsSync(join(directory, CAMPAIGN_FILE.plan))) continue;
    try {
      const fingerprint = campaignFingerprint(directory,
        [CAMPAIGN_FILE.plan, CAMPAIGN_FILE.state], [ARTIFACT_FILE.run]);
      const cached = overviewCache.get(directory);
      const fresh = cached?.fingerprint === fingerprint
        ? { plan: cached.plan, campaign: cached.campaign } : overviewCampaign(directory);
      overviewCache.set(directory, { fingerprint, ...fresh });
      campaigns.push(withInterruption(fresh.campaign, controllerInterrupted(controllerActive,
        directory, fresh.plan, fresh.campaign.status)));
    } catch (error) {
      campaigns.push({ key: entry.name, id: entry.name, title: entry.name,
        status: 'unreadable', error: errorMessage(error) });
    }
  }
  return campaigns.sort((left, right) =>
    String('updatedAt' in right ? right.updatedAt ?? '' : '')
      .localeCompare(String('updatedAt' in left ? left.updatedAt ?? '' : '')));
}

// Campaign sheet

export interface SheetFacts {
  mode: string;
  workSelection: string | null;
  repairSelection: string | null;
  repairBudget: number;
  agent: string | null;
  model: string | null;
  guidance: string | null;
  recipes: Array<{ level: number; id: string | null; version: string | null }>;
  timeLimitMinutes: number;
  spendLimitUsd: number | null;
  controllerImage: string | null;
  buildImage: string | null;
  planSha256: string;
  grading: string;
  gradingReasons: string[];
}

export interface ClimbPoint {
  score: number;
  max: number;
  level: number | null;
  unaided: boolean;
}

export interface SheetAttempt {
  id: string;
  repetition: number;
  status: string;
  phase: string;
  stalling: boolean;
  excluded: string | null;
  continued: boolean;
  logUpdatedAt: string | null;
  score: number | null;
  unaided: number | null;
  repairs: { used: number; budget: number };
  timeSec: number | null;
  spendUsd: number | null;
  climb: ClimbPoint[];
}

export interface SheetLevel {
  level: number;
  unaided: { score: number; max: number } | null;
  score: { score: number; max: number } | null;
  repairs: number;
}

export interface SheetQuestline {
  id: string;
  title: string;
  score: number | null;
  nodes: Array<{ id: string; status: string }>;
}

export interface SheetStack {
  stack: string;
  score: number | null;
  points: { score: number; max: number } | null;
  unaided: number | null;
  continued: boolean;
  repairs: { used: number; budget: number };
  regressions: number;
  timeSec: number | null;
  spendUsd: number | null;
  n: number;
  climb: ClimbPoint[];
  attempts: SheetAttempt[];
  levels: SheetLevel[] | null;
  questlines: SheetQuestline[] | null;
}

export interface CampaignSheet {
  key: string;
  id: string;
  title: string;
  status: string;
  mode: string;
  levels: number[];
  repetitions: number;
  provisional: boolean;
  mixedScope: boolean;
  executions: number;
  // A dependency campaign that stopped between executions is the one thing an
  // operator can restart; the server checks the same three facts again.
  resumable: boolean;
  createdAt: string;
  updatedAt: string;
  facts: SheetFacts;
  stacks: SheetStack[];
}

interface SheetAttemptView {
  inspected: InspectedAttempt;
  attempt: SheetAttempt;
  series: ClimbPoint[];
}

function sheetFacts(plan: CompiledCampaignPlan): SheetFacts {
  const mode = plan.definition.mode;
  const policy = plan.dependencyPolicy?.definition ?? null;
  const agent = plan.agents[0] ?? null;
  const facts = campaignFacts(plan);
  return {
    mode: mode.id,
    workSelection: policy?.workSelection ?? mode.workSelection ?? null,
    repairSelection: policy?.repair.selection ?? plan.definition.repair.selection,
    repairBudget: repairBudgetLimit(plan.definition.repair, {
      features: plan.featureCatalog?.definition.nodes.length ?? 1,
      depths: plan.definition.levels.length,
    }),
    agent: agent?.adapter ?? null,
    model: agent?.model ?? null,
    guidance: plan.attempts[0]?.guidance ?? null,
    recipes: facts.recipes,
    timeLimitMinutes: plan.definition.budgets.attemptTimeoutMinutes,
    spendLimitUsd: plan.definition.budgets.maxCostUsdPerAttempt,
    controllerImage: facts.runtime.controllerImage,
    buildImage: facts.runtime.buildImage,
    planSha256: plan.contentSha256,
    grading: gradingStatus(facts.grading),
    gradingReasons: [...new Set(facts.grading.levels.flatMap(level => level.reasons ?? []))],
  };
}

// A campaign whose levels disagree is partly publishable and says so.
function gradingStatus(grading: ReturnType<typeof campaignFacts>['grading']): string {
  const levels = new Set(grading.levels.map(level => level.status));
  return levels.size > 1 ? 'partial' : grading.status;
}

function dependencyRepairs(plan: CompiledCampaignPlan,
  dependency: DependencyProgress): { used: number; budget: number } {
  return {
    used: dependency.history?.repairAttempts ?? 0,
    budget: repairBudgetLimit(plan.definition.repair, {
      features: dependency.nodes.length,
      depths: plan.definition.levels.length,
    }),
  };
}

function attemptRegressions(attempt: InspectedAttempt): number {
  if (attempt.dependency) return attempt.dependency.regressions ?? 0;
  return (attempt.result?.levels ?? []).reduce((total, level) => total + level.regressions, 0);
}

// Continued: the attempt resumed on a repair grant, so its first grade is a
// checkpoint baseline rather than an unaided build.
function attemptContinued(attempt: InspectedAttempt): boolean {
  if (attempt.dependency) {
    return attempt.dependency.attempts.features.some(feature =>
      typeof feature.granted === 'number' && feature.granted > 0);
  }
  return (attempt.result?.levels ?? []).some(level => level.continued);
}

function sheetLevels(attempt: InspectedAttempt | null): SheetLevel[] {
  return (attempt?.result?.levels ?? []).map(level => ({
    level: level.level,
    unaided: level.firstAbort ? null : level.firstScore,
    score: level.finalScore,
    repairs: level.used,
  }));
}

function sheetQuestlines(dependency: DependencyProgress): SheetQuestline[] {
  const status = new Map(dependency.nodes.map(node => [node.id, node.status]));
  const scored = new Map((dependency.score?.questlines ?? [])
    .map(questline => [questline.id, questline.percentage ?? null]));
  return (dependency.questlines ?? []).map(questline => ({
    id: questline.id,
    title: questline.title,
    score: scored.get(questline.id) ?? null,
    nodes: questline.nodes.map(id => ({ id, status: status.get(id) ?? 'locked' })),
  }));
}

function sheetAttemptView(plan: CompiledCampaignPlan, state: CampaignAttemptState,
  directory: string, interrupted: boolean): SheetAttemptView {
  const inspected = inspectCampaignAttempt(plan, state, directory);
  const execution = inspected.execution;
  const logPath = execution
    ? join(contained(directory, execution.output, 'campaign execution'), LOG_FILE) : null;
  const log = logPath ? readTextTail(logPath) : '';
  const logUpdatedAt = logPath && existsSync(logPath)
    ? new Date(statSync(logPath).mtimeMs).toISOString() : null;
  const running = inspected.status === 'running' && !interrupted;
  const repairLimit = repairBudgetLimit(plan.definition.repair, {
    features: plan.featureCatalog?.definition.nodes.length ?? 1,
    depths: plan.definition.levels.length,
  });
  const progress = parseRunProgress(log, { repairs: repairLimit,
    running, status: inspected.status,
    dependency: plan.definition.mode?.id === 'dependency' });
  const metrics = attemptMetrics({ ...inspected, logUpdatedAt });
  const repairs = inspected.dependency ? dependencyRepairs(plan, inspected.dependency) : null;
  return {
    inspected,
    series: progress.series,
    attempt: {
      id: inspected.id,
      repetition: inspected.repetition,
      status: interrupted && inspected.status === 'running' ? 'interrupted' : inspected.status,
      phase: interrupted && inspected.status === 'running'
        ? 'Controller stopped before completion' : progress.phase,
      stalling: attemptStalling({ ...inspected, logUpdatedAt }, progress.series),
      excluded: attemptExcluded(inspected),
      continued: attemptContinued(inspected),
      logUpdatedAt,
      score: percentage(metrics?.final ?? null),
      unaided: percentage(metrics?.first ?? null),
      repairs: repairs ?? { used: metrics?.repairs ?? 0, budget: repairLimit },
      timeSec: metrics?.duration ?? null,
      spendUsd: metrics?.spend ?? null,
      climb: progress.series,
    },
  };
}

const sheetCache = new Map<string, { fingerprint: string; sheet: CampaignSheet }>();

// Facts and per-stack figures. No log text and no package walk: the climb and
// the phase come from the run output the controller already writes.
export function campaignSheet(resultsRoot: string, key: string,
  { controllerActive = campaignLockIsActive }: ViewOptions = {}): CampaignSheet {
  const directory = campaignDirectory(resultsRoot, key);
  const fingerprint = campaignFingerprint(directory, [CAMPAIGN_FILE.plan, CAMPAIGN_FILE.state],
    [ARTIFACT_FILE.run, ARTIFACT_FILE.progressionState, LOG_FILE]);
  const { plan, state } = readCampaignState(directory, { requireCurrentInputs: false });
  const interrupted = controllerInterrupted(controllerActive, directory, plan, state.status);
  const cacheKey = `${directory}:${interrupted ? 'interrupted' : 'live'}`;
  const cached = sheetCache.get(cacheKey);
  if (cached?.fingerprint === fingerprint) return cached.sheet;
  const views = state.attempts.map(attempt =>
    sheetAttemptView(plan, attempt, directory, interrupted));
  const comparison = compareCampaign<InspectedAttempt>({
    attempts: views.map(view => view.inspected) });
  const dependency = plan.definition.mode?.id === 'dependency';
  const plannedRepairLimit = repairBudgetLimit(plan.definition.repair, {
    features: plan.featureCatalog?.definition.nodes.length ?? 1,
    depths: plan.definition.levels.length,
  });
  const stacks = plan.stacks.map(stack => {
    const owned = views.filter(view => view.inspected.stack === stack.id);
    const row = comparison.rows.find(entry => entry.stack === stack.id);
    // Figures a repetition cannot average — the climb, the questline board, the
    // per-level rows — come from the newest attempt that actually ran.
    const latest = owned.findLast(view => view.inspected.execution !== null) ?? null;
    const lead = latest?.inspected ?? null;
    const repairs = lead?.dependency ? dependencyRepairs(plan, lead.dependency) : null;
    const metrics = lead ? attemptMetrics(lead) : null;
    return {
      stack: stack.id,
      score: percentage(row?.final ?? null),
      points: dependency ? uniquePoints(lead?.dependency ?? null) : metrics?.raw.final ?? null,
      unaided: percentage(row?.first ?? null),
      continued: owned.some(view => view.attempt.continued),
      repairs: repairs ?? { used: Math.round(row?.repairs ?? 0),
        budget: plannedRepairLimit },
      regressions: Math.round(median(owned.map(view => attemptRegressions(view.inspected))) ?? 0),
      timeSec: row?.duration ?? null,
      spendUsd: row?.spend ?? null,
      n: row?.n ?? 0,
      climb: latest?.series ?? [],
      attempts: owned.map(view => view.attempt),
      levels: dependency ? null : sheetLevels(lead),
      questlines: lead?.dependency ? sheetQuestlines(lead.dependency) : null,
    };
  });
  const sheet: CampaignSheet = {
    key: basename(resolve(directory)),
    id: plan.id,
    title: plan.title,
    status: interrupted ? 'attention-required' : state.status,
    mode: plan.definition.mode?.id ?? 'sequential',
    levels: plan.definition.levels,
    repetitions: plan.definition.repetitions,
    provisional: campaignFacts(plan).grading.status !== 'qualified',
    mixedScope: comparison.mixedScope,
    executions: state.summary.executions,
    resumable: dependency && state.status === 'prepared' && state.summary.executions > 0,
    createdAt: state.createdAt,
    updatedAt: state.updatedAt,
    facts: sheetFacts(plan),
    stacks,
  };
  sheetCache.set(cacheKey, { fingerprint, sheet });
  return sheet;
}

function uniquePoints(dependency: DependencyProgress | null): { score: number; max: number } | null {
  const unique = dependency?.score?.uniqueChecks;
  if (!unique || unique.passedPoints == null || unique.availablePoints == null) return null;
  return { score: unique.passedPoints, max: unique.availablePoints };
}

// Attempt sub-resources

function attemptState(directory: string, attemptId: string): CampaignAttemptState {
  const { state } = readCampaignState(directory, { requireCurrentInputs: false });
  const attempt = state.attempts.find(item => item.plan.id === attemptId);
  if (!attempt) throw new Error('campaign attempt does not exist');
  return attempt;
}

export interface AttemptCheckGrade {
  id: string;
  level: number | null;
  round: number | null;
  score: { score: number; max: number } | null;
  error?: string;
}

export interface AttemptCheck {
  key: string;
  id: string;
  description: string;
  points: number;
  feature: string;
  outcome: string;
  regressed: boolean;
  history: string[];
}

export interface AttemptChecks {
  attemptId: string;
  stack: string;
  grades: AttemptCheckGrade[];
  checks: AttemptCheck[];
}

function checkOutcome(evidence: unknown): string {
  const status = evidence !== null && typeof evidence === 'object' && 'status' in evidence
    ? (evidence as { status?: unknown }).status : null;
  if (status === 'passed') return 'pass';
  if (status === 'failed') return 'fail';
  return 'not-run';
}

function gradeDirectories(executionDirectory: string): AttemptCheckGrade[] {
  if (!existsSync(executionDirectory)) return [];
  const progression = join(executionDirectory, 'progression');
  if (existsSync(progression)) {
    return readdirSync(progression, { withFileTypes: true })
      .filter(entry => entry.isDirectory() && PROGRESSION_ATTEMPT.test(entry.name))
      .map(entry => ({ id: `progression/${entry.name}`,
        level: null, round: Number(PROGRESSION_ATTEMPT.exec(entry.name)?.[1] ?? 0), score: null }))
      .sort((left, right) => (left.round ?? 0) - (right.round ?? 0));
  }
  return readdirSync(executionDirectory, { withFileTypes: true })
    .filter(entry => entry.isDirectory() && GRADE_DIRECTORY.test(entry.name))
    .map(entry => {
      const match = GRADE_DIRECTORY.exec(entry.name);
      const level = match?.[1] ?? match?.[2] ?? null;
      return { id: entry.name, level: level === null ? null : Number(level),
        round: match?.[3] === undefined ? 0 : Number(match[3]), score: null };
    })
    .sort((left, right) => (left.level ?? 0) - (right.level ?? 0)
      || (left.round ?? 0) - (right.round ?? 0));
}

// Per-check outcome and the history of every grade that reported it: the
// question "did this ever pass" has no other answer in the evidence.
export function attemptChecks(resultsRoot: string, key: string, attemptId: string): AttemptChecks {
  const directory = campaignDirectory(resultsRoot, key);
  const attempt = attemptState(directory, attemptId);
  const execution = attempt.executions.at(-1) ?? null;
  if (!execution) return { attemptId, stack: attempt.plan.stack, grades: [], checks: [] };
  const executionDirectory = contained(directory, execution.output, 'campaign execution');
  const grades = gradeDirectories(executionDirectory);
  const checks = new Map<string, AttemptCheck>();
  grades.forEach((grade, index) => {
    const path = join(executionDirectory, grade.id, ARTIFACT_FILE.gradeBundle);
    if (!existsSync(path)) {
      grade.error = 'grade bundle is missing';
      return;
    }
    let payload;
    try {
      payload = readArtifactPayload<GradeBundlePayload>(path, { expectedKind: 'grade_bundle' });
    } catch (error) {
      grade.error = errorMessage(error);
      return;
    }
    grade.score = payload.totals?.score == null || payload.totals.max == null
      ? null : { score: payload.totals.score, max: payload.totals.max };
    for (const suite of Object.values(payload.suites ?? {})) {
      for (const feature of suite.features ?? []) {
        for (const criterion of feature.criteria ?? []) {
          const stableKey = criterion.stableKey ?? `${feature.name ?? ''}.${criterion.id ?? ''}`;
          const entry = checks.get(stableKey) ?? { key: stableKey, id: criterion.id ?? stableKey,
            description: criterionDescription(criterion), points: criterion.points ?? 0,
            feature: feature.name ?? '', outcome: 'not-run', regressed: false,
            history: grades.map(() => 'not-run') };
          entry.history[index] = checkOutcome(criterion.evidence);
          checks.set(stableKey, entry);
        }
      }
    }
  });
  for (const check of checks.values()) {
    const conclusive = check.history.filter(outcome => outcome !== 'not-run');
    check.outcome = conclusive.at(-1) ?? 'not-run';
    check.regressed = conclusive.some((outcome, index) =>
      outcome === 'fail' && conclusive.slice(0, index).includes('pass'));
  }
  return { attemptId, stack: attempt.plan.stack, grades, checks: [...checks.values()] };
}

// The grade bundle names the criterion text `desc`.
function criterionDescription(criterion: object): string {
  const record = criterion as { desc?: unknown; description?: unknown };
  if (typeof record.desc === 'string') return record.desc;
  return typeof record.description === 'string' ? record.description : '';
}

export interface AttemptPackage {
  attemptId: string;
  stack: string;
  executions: Array<{
    executionId: string;
    ordinal: number;
    status: string;
    artifacts: DashboardArtifact[];
    visuals: DashboardArtifact[];
    truncated: boolean;
  }>;
}

export function attemptPackage(resultsRoot: string, key: string,
  attemptId: string): AttemptPackage {
  const directory = campaignDirectory(resultsRoot, key);
  const attempt = attemptState(directory, attemptId);
  return {
    attemptId,
    stack: attempt.plan.stack,
    executions: attempt.executions.map(execution => {
      const scanned = walkPublicExecutionArtifacts(directory,
        contained(directory, execution.output, 'campaign execution'));
      return { executionId: execution.id, ordinal: execution.ordinal, status: execution.status,
        artifacts: scanned.artifacts,
        visuals: scanned.artifacts.filter(artifact => artifact.kind === 'visual'),
        truncated: scanned.truncated };
    }),
  };
}

export interface AttemptLogSlice {
  attemptId: string;
  from: number;
  offset: number;
  size: number;
  text: string;
}

// Bytes after an offset, so a following view pays for growth rather than for
// the whole log on every poll.
export function attemptLogSlice(resultsRoot: string, key: string, attemptId: string,
  fromOffset = 0): AttemptLogSlice {
  const directory = campaignDirectory(resultsRoot, key);
  const attempt = attemptState(directory, attemptId);
  const execution = attempt.executions.at(-1) ?? null;
  const path = execution
    ? join(contained(directory, execution.output, 'campaign execution'), LOG_FILE) : null;
  if (!path || !existsSync(path)) {
    return { attemptId, from: fromOffset, offset: 0, size: 0, text: '' };
  }
  const descriptor = openSync(path, 'r');
  try {
    const size = fstatSync(descriptor).size;
    // A rotated or truncated log invalidates the caller's offset.
    const start = Math.min(Math.max(0, fromOffset), size);
    const length = Math.min(size - start, MAX_LOG_BYTES);
    const buffer = Buffer.alloc(length);
    if (length) readSync(descriptor, buffer, 0, length, start);
    return { attemptId, from: fromOffset, offset: start + length, size,
      text: redactCredentials(buffer.toString('utf8')) };
  } finally {
    closeSync(descriptor);
  }
}

// Campaign progression

export interface ProgressionCatalogNode {
  id: string;
  title: string;
  questline: string;
  depth: number;
  dependencies: string[];
}

export interface ProgressionStep {
  sequence: number;
  action: 'build' | 'repair' | 'grant';
  targets: string[];
  // Node status after the event, index-aligned with `nodes`.
  statuses: string[];
  score: number | null;
  repairs: number;
}

export interface ProgressionTrack {
  stack: string;
  attemptId: string;
  updatedAt: string;
  steps: ProgressionStep[];
}

export interface CampaignProgression {
  key: string;
  depths: number[];
  questlines: Array<{ id: string; title: string; nodes: string[] }>;
  nodes: ProgressionCatalogNode[];
  stacks: ProgressionTrack[];
}

function progressionSnapshot(state: ProgressionState, nodeIds: readonly string[]): {
  statuses: string[];
  score: number | null;
  repairs: number;
} {
  const average = progressionEngine.score(state).questlineAveragePercentage;
  return {
    statuses: nodeIds.map(id => state.nodes[id]?.status ?? 'locked'),
    score: average == null ? null : Math.round(average * 10) / 10,
    repairs: state.attempts.filter(attempt => attempt.repair !== undefined).length,
  };
}

function progressionSteps(state: DependencyState, nodeIds: readonly string[]): ProgressionStep[] {
  let replay = progressionEngine.initialize(state.definition);
  return state.events.map(event => {
    const action = progressionEngine.nextAction(replay);
    if (event.type === 'repairs-granted') {
      replay = progressionEngine.grantRepairs(replay, event.grant);
      return { sequence: event.sequence, action: 'grant' as const,
        targets: [...event.grant.nodeIds], ...progressionSnapshot(replay, nodeIds) };
    }
    const targets = action.type === 'terminal'
      ? [] : [...(action.prompt as DependencyPromptSelection).nodeIds];
    const repair = action.type === 'repair';
    replay = progressionEngine.recordResult(replay, event.result);
    return { sequence: event.sequence, action: repair ? 'repair' as const : 'build' as const,
      targets, ...progressionSnapshot(replay, nodeIds) };
  });
}

const progressionCache = new Map<string, { fingerprint: string; view: CampaignProgression }>();

// The catalog subgraph the campaign runs, plus one node-status snapshot per
// progression event: the graph and its replay come from the same read.
export function campaignProgression(resultsRoot: string, key: string): CampaignProgression | null {
  const directory = campaignDirectory(resultsRoot, key);
  const { plan, state } = readCampaignState(directory, { requireCurrentInputs: false });
  if (plan.definition.mode?.id !== 'dependency' || !plan.featureCatalog
    || !plan.dependencyPolicy) return null;
  const fingerprint = campaignFingerprint(directory, [CAMPAIGN_FILE.plan, CAMPAIGN_FILE.state],
    [ARTIFACT_FILE.progressionState]);
  const cached = progressionCache.get(directory);
  if (cached?.fingerprint === fingerprint) return cached.view;
  const progression = compileProgressionInput(dependencyRuntimeDefinition(
    plan.featureCatalog, plan.dependencyPolicy));
  const definition = progression.definition;
  const nodeIds = definition.nodes.map(node => node.id);
  const owned = new Set(nodeIds);
  const stacks: ProgressionTrack[] = [];
  for (const attempt of state.attempts) {
    const execution = attempt.executions.at(-1) ?? null;
    if (!execution) continue;
    const path = join(contained(directory, execution.output, 'campaign execution'),
      ARTIFACT_FILE.progressionState);
    if (!existsSync(path)) continue;
    const stored = readProgressionState(path, {
      progression,
      featureCatalogIdentity: plan.featureCatalog.identity,
      dependencyPolicyIdentity: plan.dependencyPolicy.identity,
      owner: campaignProgressionOwner(plan, attempt.plan, { workspace: true }),
    });
    stacks.push({ stack: attempt.plan.stack, attemptId: attempt.plan.id,
      updatedAt: new Date(statSync(path).mtimeMs).toISOString(),
      steps: progressionSteps(stored.state as DependencyState, nodeIds) });
  }
  const view: CampaignProgression = {
    key: basename(resolve(directory)),
    depths: [...new Set(definition.nodes.map(node => node.level))].sort((a, b) => a - b),
    questlines: definition.questlines.map(questline => ({ id: questline.id,
      title: questline.title, nodes: [...questline.nodes] })),
    nodes: definition.nodes.map(node => ({ id: node.id, title: node.title,
      questline: node.questline, depth: node.level,
      dependencies: node.dependencies.filter(id => owned.has(id)) })),
    stacks,
  };
  progressionCache.set(directory, { fingerprint, view });
  return view;
}
