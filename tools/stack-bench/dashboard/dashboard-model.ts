import { closeSync, existsSync, fstatSync, openSync, readFileSync, readSync, readdirSync,
  lstatSync, realpathSync, statSync,
} from 'node:fs';
import { basename, dirname, extname, join, relative, resolve, sep } from 'node:path';

import type { CampaignAttemptPlan, CompiledCampaignPlan }
  from '../src/campaigns/campaign-compiler.mjs';
import type { CampaignAttemptState } from '../src/campaigns/campaign-scheduler.mjs';
import { readArtifact, readArtifactPayload } from '../src/evidence/artifacts.mjs';
import { compileCampaignFile, validateCompiledCampaignPlan } from '../src/campaigns/campaign-compiler.mjs';
import { campaignLockIsActive } from '../src/campaigns/campaign-lock.mjs';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.mjs';
import { validateCampaignState } from '../src/campaigns/campaign-scheduler.mjs';
import { dependencyProgress } from '../src/campaigns/campaign-inspection.mjs';
import { redactCredentials } from '../src/evidence/diagnostic-sanitizer.mjs';

const MAX_LOG_BYTES = 96 * 1024;
const MAX_PUBLIC_TEXT_BYTES = 8 * 1024 * 1024;
const MAX_ARTIFACTS_PER_EXECUTION = 512;
const IMAGE_TYPES = new Map([
  ['.png', 'image/png'], ['.jpg', 'image/jpeg'], ['.jpeg', 'image/jpeg'], ['.webp', 'image/webp'],
]);
const CAMPAIGN_ARTIFACT = /^(?:plan\.json|state\.json|report\/report\.(?:html|json))$/;
const EXECUTION_ARTIFACT = /^(?:run\.json|preflight\.json|recovery\.json|progression-state\.json|process\.(?:stdout|stderr)\.log|backend\.log|level-l\d+-checkpoint\.json|progression\/attempt-\d+\/(?:bundle\.json|contract-lint\.json|actions\.json|grading-[^/]+\.json|failure-media\/[^/]+\.(?:png|jpe?g|webp))|(?:first-build-l\d+-grading|l\d+-fix\d+-grading|grading)\/(?:bundle\.json|contract-lint\.json|actions\.json|grading-[^/]+\.json|failure-media\/[^/]+\.(?:png|jpe?g|webp)))$/i;

type ControllerActive = (directory: string, campaign: CompiledCampaignPlan) => boolean;

export interface DashboardArtifact {
  id: string;
  path: string;
  name: string;
  kind: 'visual' | 'report' | 'log' | 'data';
  contentType: string;
  size: number;
}

export interface ResolvedDashboardArtifact extends DashboardArtifact {
  absolute: string;
}

interface Score {
  score: number;
  max: number;
}

interface CheckFailure {
  stableKey?: string;
  description?: string;
}

interface RunOutcome {
  kind?: string;
  phase?: string;
  reason?: string | null;
  appFailures?: Array<string | CheckFailure>;
}

interface RunLevel {
  level: number;
  score: number;
  max: number;
  graded?: boolean;
  durationSec?: number | null;
  buildCostUsd?: number | null;
  fixCostUsd?: number | null;
  firstBuild?: Score & { outcome?: RunOutcome; missed?: Array<string | CheckFailure> };
  repair?: { roundsUsed?: number; status?: string | null };
  outcome?: RunOutcome;
  missed?: Array<string | CheckFailure>;
}

interface BenchmarkRunPayload {
  outcome?: RunOutcome;
  totals?: Score & { costUsd?: number | null; durationSec?: number | null };
  backendLease?: { state?: string | null };
  levels?: RunLevel[];
}

interface DashboardRunResult {
  unreadable?: string;
  outcome?: string;
  outcomePhase?: string | null;
  outcomeReason?: string | null;
  score?: Score | null;
  costUsd?: number | null;
  durationSec?: number | null;
  cleanup?: string | null;
  levels?: Array<Record<string, unknown>>;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function contained(root: string, path: string, label: string): string {
  const absoluteRoot = resolve(root);
  const absolute = resolve(absoluteRoot, path);
  const rel = relative(absoluteRoot, absolute);
  if (rel === '..' || rel.startsWith(`..${sep}`) || rel === '') {
    throw new Error(`${label} is outside the configured dashboard root`);
  }
  return absolute;
}

function readTextTail(path: string, limit = MAX_LOG_BYTES): string {
  if (!existsSync(path)) return '';
  const descriptor = openSync(path, 'r');
  try {
    const size = fstatSync(descriptor).size;
    const length = Math.min(size, limit);
    const buffer = Buffer.alloc(length);
    readSync(descriptor, buffer, 0, length, size - length);
    return redactCredentials(buffer.toString('utf8'));
  } finally {
    closeSync(descriptor);
  }
}

function artifactId(relativePath: string): string {
  return Buffer.from(relativePath, 'utf8').toString('base64url');
}

function artifactLabel(path: string): string {
  const name = basename(path);
  if (path === 'plan.json') return 'Frozen plan';
  if (path === 'state.json') return 'Campaign state';
  if (path === 'report/report.html') return 'Campaign report';
  if (path === 'report/report.json') return 'Report data';
  if (name === 'run.json') return 'Run result';
  if (name === 'preflight.json') return 'Preflight result';
  if (name === 'recovery.json') return 'Recovery record';
  if (name === 'progression-state.json') return 'Dependency progress';
  if (name === 'process.stdout.log') return 'Run output';
  if (name === 'process.stderr.log') return 'Run errors';
  if (name === 'backend.log') return 'Backend output';
  if (name === 'bundle.json') return `${basename(dirname(path))} bundle`;
  if (name === 'actions.json') return `${basename(dirname(path))} actions`;
  if (name === 'contract-lint.json') return `${basename(dirname(path))} contract check`;
  return name.replace(/[-_]/g, ' ');
}

function artifactMetadata(campaignDirectory: string, path: string): DashboardArtifact {
  const absolute = contained(campaignDirectory, path, 'campaign artifact');
  const size = statSync(absolute).size;
  const extension = extname(path).toLowerCase();
  const kind = IMAGE_TYPES.has(extension) ? 'visual'
    : path.endsWith('/report.html') ? 'report'
      : path.endsWith('.log') ? 'log' : 'data';
  return { id: artifactId(path), path: path.replaceAll('\\', '/'), name: artifactLabel(path),
    kind, contentType: IMAGE_TYPES.get(extension) ?? (kind === 'report' ? 'text/html' : 'text/plain'),
    size };
}

function rejectSymlinkPath(root: string, path: string): void {
  const rel = relative(resolve(root), resolve(path));
  let current = resolve(root);
  for (const segment of rel.split(sep)) {
    current = join(current, segment);
    if (lstatSync(current).isSymbolicLink()) {
      throw new Error('campaign artifact path contains a symbolic link');
    }
  }
}

function walkPublicExecutionArtifacts(campaignDirectory: string, executionDirectory: string): {
  artifacts: DashboardArtifact[];
  truncated: boolean;
} {
  const found: DashboardArtifact[] = [];
  let truncated = false;
  const visit = (directory: string): void => {
    const directoryRelative = relative(executionDirectory, directory).replaceAll('\\', '/');
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      if (found.length >= MAX_ARTIFACTS_PER_EXECUTION) {
        truncated = true;
        return;
      }
      if (entry.isSymbolicLink()) continue;
      const absolute = join(directory, entry.name);
      if (entry.isDirectory()) {
        const allowed = directoryRelative === ''
          ? /^(?:first-build-l\d+-grading|l\d+-fix\d+-grading|grading|progression)$/i.test(entry.name)
          : directoryRelative === 'progression'
            ? /^attempt-\d+$/i.test(entry.name)
          : (/^(?:progression\/attempt-\d+|(?:.*\/)?(?:first-build-l\d+-grading|l\d+-fix\d+-grading|grading))$/i
            .test(directoryRelative) && entry.name === 'failure-media');
        if (allowed) visit(absolute);
        if (truncated) return;
      }
      else if (entry.isFile()) {
        const executionRelative = relative(executionDirectory, absolute).replaceAll('\\', '/');
        if (EXECUTION_ARTIFACT.test(executionRelative)) {
          const campaignRelative = relative(campaignDirectory, absolute).replaceAll('\\', '/');
          found.push(artifactMetadata(campaignDirectory, campaignRelative));
        }
      }
    }
  };
  if (existsSync(executionDirectory)) visit(executionDirectory);
  return { artifacts: found.sort((left, right) => left.path.localeCompare(right.path)), truncated };
}

function campaignPackage(campaignDirectory: string, attempts: CampaignAttemptState[]) {
  const campaign = ['plan.json', 'state.json', 'report/report.html', 'report/report.json']
    .filter(path => existsSync(join(campaignDirectory, path)))
    .map(path => artifactMetadata(campaignDirectory, path));
  const executions: Array<{
    attemptId: string;
    stack: string;
    executionId: string;
    ordinal: number;
    status: string;
    artifacts: DashboardArtifact[];
    visuals: DashboardArtifact[];
    truncated: boolean;
  }> = [];
  for (const attempt of attempts) {
    for (const execution of attempt.executions) {
      const directory = contained(campaignDirectory, execution.output, 'campaign execution');
      const scanned = walkPublicExecutionArtifacts(campaignDirectory, directory);
      const artifacts = scanned.artifacts;
      executions.push({ attemptId: attempt.plan.id, stack: attempt.plan.stack,
        executionId: execution.id, ordinal: execution.ordinal, status: execution.status,
        artifacts, visuals: artifacts.filter(artifact => artifact.kind === 'visual'),
        truncated: scanned.truncated });
    }
  }
  return { campaign, executions };
}

export function resolveCampaignArtifact(resultsRoot: string, key: string,
  id: string): ResolvedDashboardArtifact {
  if (!/^[a-z0-9][a-z0-9.-]*$/.test(key)) throw new Error('campaign key is invalid');
  let path;
  try { path = Buffer.from(id, 'base64url').toString('utf8'); }
  catch { throw new Error('campaign artifact id is invalid'); }
  if (!path || artifactId(path) !== id || path.includes('\\') || path.startsWith('/')) {
    throw new Error('campaign artifact id is invalid');
  }
  const executionMatch = path.match(/^attempts\/([^/]+)\/(execution-\d+)\/(.+)$/);
  const allowed = CAMPAIGN_ARTIFACT.test(path)
    || (executionMatch !== null && EXECUTION_ARTIFACT.test(executionMatch[3] ?? ''));
  if (!allowed) {
    throw new Error('campaign artifact is not available in the dashboard');
  }
  const campaignsRoot = join(resolve(resultsRoot), 'campaigns');
  const campaignDirectory = contained(campaignsRoot, key, 'campaign');
  const absolute = contained(campaignDirectory, path, 'campaign artifact');
  if (!existsSync(absolute) || !statSync(absolute).isFile()) throw new Error('campaign artifact does not exist');
  rejectSymlinkPath(campaignsRoot, absolute);
  const realCampaign = realpathSync(campaignDirectory);
  const realArtifact = realpathSync(absolute);
  contained(realCampaign, relative(realCampaign, realArtifact), 'campaign artifact');
  return { ...artifactMetadata(campaignDirectory, path), absolute };
}

export function readCampaignArtifactBody(artifact: ResolvedDashboardArtifact): Buffer {
  if (artifact.kind === 'visual') return readFileSync(artifact.absolute);
  if (artifact.size > MAX_PUBLIC_TEXT_BYTES) throw new Error('campaign artifact is too large to view');
  return Buffer.from(redactCredentials(readFileSync(artifact.absolute, 'utf8')));
}

function matches(text: string, pattern: RegExp): Array<RegExpMatchArray & { index: number }> {
  return [...text.matchAll(pattern)].map(match => Object.assign(match, { index: match.index ?? 0 }));
}

export function parseRunProgress(log: string, { fixRounds = 0, running = true, status = null }: {
  fixRounds?: number;
  running?: boolean;
  status?: string | null;
} = {}) {
  const totals = matches(log, /^\s*TOTAL\b.*?(\d+)\/(\d+)\s*$/gm)
    .map(match => ({ index: match.index, score: Number(match[1]), max: Number(match[2]) }));
  const roundMarkers = matches(log, /^--- fix round (\d+)\/(\d+) ---$/gm)
    .map(match => ({ index: match.index, round: Number(match[1]), budget: Number(match[2]) }));
  const grading = matches(log, /^===\s+[^\n]*?-l(\d+)(?:-(?:first|fix(\d+)))?\s+\([^\n]+\)\s*===$/gm)
    .map(match => ({ index: match.index, level: Number(match[1]),
      round: match[2] ? Number(match[2]) : 0 }));
  const latestTotal = totals.at(-1) ?? null;
  const latestRound = roundMarkers.at(-1) ?? null;
  const latestGrading = grading.at(-1) ?? null;
  const latestIndex = Math.max(latestTotal?.index ?? -1, latestRound?.index ?? -1,
    latestGrading?.index ?? -1);
  let phase = status === 'pending' ? 'Waiting to start'
    : running ? 'Building the generated app' : 'Finished';
  let level = latestGrading?.level ?? null;
  let round = latestRound?.round ?? latestGrading?.round ?? 0;
  const budget = latestRound?.budget ?? fixRounds;
  if (latestIndex === latestGrading?.index) {
    phase = latestGrading.round
      ? `Grading L${latestGrading.level} after repair ${latestGrading.round} of ${budget}`
      : `Grading the first L${latestGrading.level} build`;
  } else if (latestIndex === latestRound?.index) {
    phase = `Repairing L${latestGrading?.level ?? 1} · round ${latestRound.round} of ${latestRound.budget}`;
  } else if (latestIndex === latestTotal?.index && running) {
    phase = 'Preparing the next step';
  }
  return {
    phase,
    level,
    repair: { round, budget },
    firstScore: totals[0] ? { score: totals[0].score, max: totals[0].max } : null,
    latestScore: latestTotal ? { score: latestTotal.score, max: latestTotal.max } : null,
    completedGrades: totals.length,
    // Every completed grade in order — the attempt's trajectory. A view can
    // draw the climb, and a flat tail is the stall an operator otherwise
    // discovers by diffing round logs.
    series: totals.map(total => ({ score: total.score, max: total.max })),
  };
}

// A first grade that never ran is not a zero. The run records the phase that
// stopped it (for example application-seed); surfacing that keeps an aborted
// grade from being averaged into first-build scores as if the app scored 0.
export function firstGradeAbort(firstBuild: (Score & { outcome?: RunOutcome }) | null | undefined): {
  phase: string;
  reason: string | null;
} | null {
  const outcome = firstBuild?.outcome;
  if (!outcome || outcome.kind === 'passed') return null;
  if (!outcome.phase || outcome.phase === 'grading') return null;
  return { phase: outcome.phase, reason: outcome.reason ?? null };
}

function readRun(path: string): DashboardRunResult | null {
  if (!existsSync(path)) return null;
  try {
    const run = readArtifactPayload<BenchmarkRunPayload>(path, { expectedKind: 'benchmark_run' });
    return {
      outcome: run.outcome?.kind ?? 'ungraded',
      outcomePhase: run.outcome?.phase ?? null,
      outcomeReason: run.outcome?.reason ?? null,
      score: run.totals ? { score: run.totals.score, max: run.totals.max } : null,
      costUsd: run.totals?.costUsd ?? null,
      durationSec: run.totals?.durationSec ?? null,
      cleanup: run.backendLease?.state ?? null,
      levels: (run.levels ?? []).map(level => ({
        level: level.level,
        firstScore: level.firstBuild ? { score: level.firstBuild.score, max: level.firstBuild.max } : null,
        firstAbort: firstGradeAbort(level.firstBuild),
        finalScore: level.graded ? { score: level.score, max: level.max } : null,
        roundsUsed: level.repair?.roundsUsed ?? 0,
        repairStatus: level.repair?.status ?? null,
        outcome: level.outcome?.kind ?? null,
        durationSec: level.durationSec ?? null,
        // Level records carry spend as it is incurred, while run.totals only
        // appears once an attempt finishes. Reporting both lets a view show
        // what a still-running attempt has already cost.
        costUsd: level.buildCostUsd == null && level.fixCostUsd == null
          ? null : (level.buildCostUsd ?? 0) + (level.fixCostUsd ?? 0),
        // The FINAL failing set. level.outcome.appFailures is authoritative for
        // a graded level; the older fallbacks describe the first build and must
        // not be shown as "still failing" on an attempt that repaired them.
        failures: (level.outcome?.appFailures
          ?? (level.graded ? [] : level.missed ?? level.firstBuild?.missed ?? [])).map(item =>
          typeof item === 'string' ? item : item?.stableKey ?? item?.description ?? 'Failed check'),
      })),
    };
  } catch (error) {
    return { unreadable: errorMessage(error) };
  }
}

// Derive operator-facing facts from the compiled plan so they cannot drift
// from the campaign that ran.
interface CampaignFactsPlan {
  attempts?: Array<{
    condition?: {
      requested?: {
        levels?: Array<{
          level: number;
          recipe?: { id?: string; version?: string };
        }>;
      };
    };
  }>;
  agents?: CompiledCampaignPlan['agents'];
  definition?: {
    mode?: { id?: string };
    runtime?: { controllerImage?: string | null; buildImage?: string | null };
  };
}

export function campaignFacts(plan: CampaignFactsPlan) {
  const requested = plan.attempts?.[0]?.condition?.requested?.levels;
  return {
    mode: plan.definition?.mode?.id ?? 'sequential',
    agents: (plan.agents ?? []).map(agent => ({
      adapter: agent.adapter ?? agent.identity?.id ?? null,
      version: agent.adapterVersion ?? agent.identity?.version ?? null,
      model: agent.model ?? null,
    })),
    recipes: Array.isArray(requested) ? requested.map(level => ({
      level: level.level,
      id: level.recipe?.id ?? null,
      version: level.recipe?.version ?? null,
    })) : [],
    runtime: plan.definition?.runtime ? {
      controllerImage: plan.definition.runtime.controllerImage ?? null,
      buildImage: plan.definition.runtime.buildImage ?? null,
    } : null,
  };
}

function readDashboardCampaignState(directory: string) {
  const plan = validateCompiledCampaignPlan(
    readArtifact(join(directory, 'plan.json'), { expectedKind: 'campaign_plan' }).payload,
    { requireCurrentInputs: false });
  const state = validateCampaignState(
    readArtifact(join(directory, 'state.json'), { expectedKind: 'campaign_state' }).payload);
  if (state.campaignId !== plan.id || state.campaignSha256 !== plan.contentSha256
    || state.maxParallel !== plan.summary.parallelism
    || canonicalDefinitionJson(state.attempts.map(attempt => attempt.plan))
      !== canonicalDefinitionJson(plan.attempts)) {
    throw new Error('stored campaign state does not match its compiled plan');
  }
  return { plan, state };
}

function summarizeAttempt(plan: CompiledCampaignPlan, attempt: CampaignAttemptState,
  campaignDirectory: string, fixRounds: number, { includeLog = false }: {
    includeLog?: boolean;
  } = {}) {
  const execution = attempt.executions.at(-1) ?? null;
  let executionDirectory = null;
  let log = '';
  let run = null;
  let logUpdatedAt = null;
  if (execution) {
    executionDirectory = contained(campaignDirectory, execution.output, 'campaign execution');
    const logPath = join(executionDirectory, 'process.stdout.log');
    log = readTextTail(logPath);
    // When the run last wrote anything. A running attempt whose output has
    // been silent for a long time is wedged in a way no score can show.
    if (existsSync(logPath)) logUpdatedAt = new Date(statSync(logPath).mtimeMs).toISOString();
    run = readRun(join(executionDirectory, 'run.json'));
  }
  const progress = parseRunProgress(log, { fixRounds, running: attempt.status === 'running',
    status: attempt.status });
  if (run?.score) progress.latestScore = run.score;
  return {
    id: attempt.plan.id,
    stack: attempt.plan.stack,
    model: attempt.plan.model,
    guidance: attempt.plan.guidance,
    repetition: attempt.plan.repetition,
    levels: attempt.plan.levels,
    status: attempt.status,
    execution: execution ? {
      id: execution.id,
      ordinal: execution.ordinal,
      status: execution.status,
      outcome: execution.outcome,
      reason: execution.reason,
      startedAt: execution.startedAt,
      completedAt: execution.completedAt,
      runIndex: execution.runIndex ?? null,
    } : null,
    progress,
    logUpdatedAt,
    result: run,
    dependency: dependencyProgress(plan, attempt.plan, executionDirectory),
    ...(includeLog ? { log: log.split(/\r?\n/).slice(-160).join('\n') } : {}),
  };
}

export function summarizeCampaign(directory: string, {
  includeLogs = false,
  includePackage = false,
  controllerActive = null,
}: {
  includeLogs?: boolean;
  includePackage?: boolean;
  controllerActive?: ControllerActive | null;
} = {}) {
  const { plan, state } = readDashboardCampaignState(directory);
  let attempts = state.attempts.map(attempt => summarizeAttempt(plan, attempt, directory,
    plan.definition.budgets.fixRounds, { includeLog: includeLogs }));
  const interrupted = state.status === 'running' && controllerActive !== null
    && !controllerActive(directory, plan);
  if (interrupted) {
    attempts = attempts.map(attempt => attempt.status !== 'running' ? attempt : ({
      ...attempt,
      status: 'interrupted',
      execution: attempt.execution ? { ...attempt.execution, status: 'interrupted' } : null,
      progress: { ...attempt.progress, phase: 'Controller stopped before completion' },
    }));
  }
  return {
    key: basename(resolve(directory)),
    id: plan.id,
    version: plan.version,
    sha256: plan.contentSha256,
    title: plan.title,
    state: plan.state,
    mode: plan.definition.mode?.id ?? 'sequential',
    status: interrupted ? 'attention-required' : state.status,
    track: plan.definition.track,
    levels: plan.definition.levels,
    stacks: plan.stacks.map(stack => stack.id),
    repetitions: plan.definition.repetitions,
    maxParallel: state.maxParallel,
    createdAt: state.createdAt,
    updatedAt: state.updatedAt,
    summary: interrupted ? { ...state.summary, interrupted: state.summary.running, running: 0 }
      : state.summary,
    interrupted,
    ...(interrupted ? { statusReason: 'The campaign controller is no longer running.' } : {}),
    budgets: plan.definition.budgets,
    facts: campaignFacts(plan),
    attempts,
    ...(includePackage ? { package: campaignPackage(directory, state.attempts) } : {}),
  };
}

export function discoverCampaigns(campaignsRoot: string, {
  includeLogs = false,
  controllerActive = campaignLockIsActive,
}: { includeLogs?: boolean; controllerActive?: ControllerActive } = {}) {
  if (!existsSync(campaignsRoot)) return [];
  const campaigns: Array<ReturnType<typeof summarizeCampaign> | {
    key: string;
    id: string;
    title: string;
    status: 'unreadable';
    error: string;
    attempts: [];
  }> = [];
  for (const entry of readdirSync(campaignsRoot, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const directory = join(campaignsRoot, entry.name);
    if (!existsSync(join(directory, 'state.json')) || !existsSync(join(directory, 'plan.json'))) continue;
    try {
      campaigns.push(summarizeCampaign(directory, { includeLogs, controllerActive }));
    } catch (error) {
      campaigns.push({ key: entry.name, id: entry.name, title: entry.name,
        status: 'unreadable', error: errorMessage(error), attempts: [] });
    }
  }
  return campaigns.sort((left, right) => String('updatedAt' in right ? right.updatedAt ?? '' : '')
    .localeCompare(String('updatedAt' in left ? left.updatedAt ?? '' : '')));
}

export interface DashboardPlan {
  id: string;
  version?: string;
  title: string;
  state: string;
  mode?: string;
  track?: string;
  levels?: number[];
  stacks?: string[];
  attempts?: number;
  parallelism?: number;
  budgets?: CompiledCampaignPlan['definition']['budgets'];
  sha256?: string;
  file: string;
  error?: string;
}

export function discoverPlans(plansRoot: string): DashboardPlan[] {
  if (!existsSync(plansRoot)) return [];
  const plans: DashboardPlan[] = [];
  for (const entry of readdirSync(plansRoot, { withFileTypes: true })) {
    if (!entry.isFile() || !entry.name.endsWith('.json')) continue;
    const path = join(plansRoot, entry.name);
    try {
      const plan = compileCampaignFile(path);
      plans.push({ id: plan.id, version: plan.version, title: plan.title, state: plan.state,
        mode: plan.definition.mode?.id ?? 'sequential',
        track: plan.definition.track, levels: plan.definition.levels,
        stacks: plan.stacks.map(stack => stack.id), attempts: plan.summary.attempts,
        parallelism: plan.summary.parallelism, budgets: plan.definition.budgets,
        sha256: plan.contentSha256, file: entry.name });
    } catch (error) {
      plans.push({ id: entry.name.slice(0, -5), title: entry.name, state: 'invalid',
        error: errorMessage(error), file: entry.name });
    }
  }
  return plans.sort((left, right) => left.title.localeCompare(right.title));
}

export function readDashboardOverview({
  resultsRoot,
  plansRoot,
  operations = [],
  controllerActive = campaignLockIsActive,
}: {
  resultsRoot: string;
  plansRoot: string;
  operations?: unknown[];
  controllerActive?: ControllerActive;
}) {
  const campaignsRoot = join(resolve(resultsRoot), 'campaigns');
  const campaigns = discoverCampaigns(campaignsRoot, { controllerActive });
  const plans = discoverPlans(plansRoot);
  const counts = {
    running: campaigns.filter(campaign => campaign.status === 'running').length,
    completed: campaigns.filter(campaign => campaign.status === 'completed').length,
    attention: campaigns.filter(campaign => ['attention-required', 'unreadable'].includes(campaign.status)).length,
    prepared: campaigns.filter(campaign => campaign.status === 'prepared').length,
  };
  return { schemaVersion: 1, generatedAt: new Date().toISOString(), counts, campaigns, plans,
    operations };
}

export function campaignDetail(resultsRoot: string, key: string, {
  controllerActive = campaignLockIsActive,
}: { controllerActive?: ControllerActive } = {}) {
  if (!/^[a-z0-9][a-z0-9.-]*$/.test(key)) throw new Error('campaign key is invalid');
  return summarizeCampaign(contained(join(resolve(resultsRoot), 'campaigns'), key, 'campaign'),
    { includeLogs: true, includePackage: true, controllerActive });
}

export function readJsonLines<T = unknown>(path: string): T[] {
  if (!existsSync(path)) return [];
  const lines = readFileSync(path, 'utf8').split(/\r?\n/);
  const last = lines.findLastIndex(line => line.trim() !== '');
  const events: T[] = [];
  for (let index = 0; index <= last; index += 1) {
    const line = lines[index];
    if (!line?.trim()) continue;
    try { events.push(JSON.parse(line) as T); }
    catch {
      if (index === last) break;
      throw new Error(`dashboard operation feed line ${index + 1} is invalid JSON`);
    }
  }
  return events;
}
