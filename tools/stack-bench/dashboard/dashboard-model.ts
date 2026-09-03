import { closeSync, existsSync, fstatSync, openSync, readFileSync, readSync, readdirSync,
  lstatSync, realpathSync, statSync,
} from 'node:fs';
import { basename, dirname, extname, join, relative, resolve, sep } from 'node:path';

import type { CompiledCampaignPlan }
  from '../src/campaigns/campaign-compiler.js';
import type { CampaignAttemptState } from '../src/campaigns/campaign-scheduler.js';
import { ARTIFACT_FILE } from '../src/evidence/artifacts.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { campaignLockIsActive } from '../src/campaigns/campaign-lock.js';
import { readCampaignState } from '../src/campaigns/campaign-scheduler.js';
import { campaignFacts, inspectCampaignAttempt } from '../src/campaigns/campaign-inspection.js';
import { redactCredentials } from '../src/evidence/diagnostic-sanitizer.js';
import { CAMPAIGN_FILE } from '../src/campaigns/campaign-path.js';
import { repairBudgetLimit } from '../src/progression/repair-plan.js';

export const MAX_LOG_BYTES = 96 * 1024;
const MAX_PUBLIC_TEXT_BYTES = 8 * 1024 * 1024;
const MAX_ARTIFACTS_PER_EXECUTION = 512;
const IMAGE_TYPES = new Map([
  ['.png', 'image/png'], ['.jpg', 'image/jpeg'], ['.jpeg', 'image/jpeg'], ['.webp', 'image/webp'],
]);
const CAMPAIGN_ARTIFACT = /^(?:plan\.json|state\.json|report\/report\.(?:html|json))$/;
const EXECUTION_ARTIFACT = /^(?:run\.json|preflight\.json|recovery\.json|progression-state\.json|process\.json|process\.(?:stdout|stderr)\.log|backend\.log|level-l\d+-checkpoint\.json|progression\/attempt-\d+\/(?:bundle\.json|contract-lint\.json|actions\.json|grading-[^/]+\.json|failure-media\/[^/]+\.(?:png|jpe?g|webp))|(?:first-build-l\d+-grading|l\d+-fix\d+-grading|grading)\/(?:bundle\.json|contract-lint\.json|actions\.json|grading-[^/]+\.json|failure-media\/[^/]+\.(?:png|jpe?g|webp)))$/i;

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

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function contained(root: string, path: string, label: string): string {
  const absoluteRoot = resolve(root);
  const absolute = resolve(absoluteRoot, path);
  const rel = relative(absoluteRoot, absolute);
  if (rel === '..' || rel.startsWith(`..${sep}`) || rel === '') {
    throw new Error(`${label} is outside the configured dashboard root`);
  }
  return absolute;
}

export function readTextTail(path: string, limit = MAX_LOG_BYTES): string {
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
  if (path === CAMPAIGN_FILE.plan) return 'Frozen plan';
  if (path === CAMPAIGN_FILE.state) return 'Campaign state';
  if (path === `report/${CAMPAIGN_FILE.reportHtml}`) return 'Campaign report';
  if (path === `report/${CAMPAIGN_FILE.reportJson}`) return 'Report data';
  if (name === ARTIFACT_FILE.run) return 'Run result';
  if (name === ARTIFACT_FILE.preflight) return 'Preflight result';
  if (name === ARTIFACT_FILE.recovery) return 'Recovery record';
  if (name === ARTIFACT_FILE.progressionState) return 'Dependency progress';
  if (name === 'process.stdout.log') return 'Run output';
  if (name === 'process.stderr.log') return 'Run errors';
  if (name === 'backend.log') return 'Backend output';
  if (name === ARTIFACT_FILE.gradeBundle) return `${basename(dirname(path))} bundle`;
  if (name === ARTIFACT_FILE.actions) return `${basename(dirname(path))} actions`;
  if (name === ARTIFACT_FILE.contractLint) return `${basename(dirname(path))} contract check`;
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

export function walkPublicExecutionArtifacts(campaignDirectory: string, executionDirectory: string): {
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
  const campaign = [CAMPAIGN_FILE.plan, CAMPAIGN_FILE.state,
    `report/${CAMPAIGN_FILE.reportHtml}`, `report/${CAMPAIGN_FILE.reportJson}`]
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

export function parseRunProgress(log: string, { repairs = 0, running = true, status = null,
  dependency = false }: {
  repairs?: number;
  running?: boolean;
  status?: string | null;
  dependency?: boolean;
} = {}) {
  const totals = matches(log, /^\s*TOTAL\b.*?(\d+)\/(\d+)\s*$/gm)
    .map(match => ({ index: match.index, score: Number(match[1]), max: Number(match[2]) }));
  const roundMarkers = matches(log,
    /^--- (?:feature )?repair (\d+)\/(\d+)(?:: (.+))? ---$/gm)
    .map(match => ({ index: match.index, round: Number(match[1]), budget: Number(match[2]),
      target: match[3] ?? null }));
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
  const level = latestGrading?.level ?? null;
  const round = latestRound?.round ?? latestGrading?.round ?? 0;
  const budget = latestRound?.budget ?? repairs;
  const target = latestRound?.target ? ` for ${latestRound.target}` : '';
  const stage = (value: number): string => dependency ? `depth ${value}` : `L${value}`;
  if (latestIndex === latestGrading?.index) {
    phase = latestGrading.round
      ? `Grading ${stage(latestGrading.level)} after repair ${round} of ${budget}${target}`
      : `Grading the first ${stage(latestGrading.level)} build`;
  } else if (latestIndex === latestRound?.index) {
    phase = latestRound.target
      ? `Repairing ${latestRound.target} · ${latestRound.round} of ${latestRound.budget}`
      : `Repairing ${stage(latestGrading?.level ?? 1)} · round ${latestRound.round} of ${latestRound.budget}`;
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
    // Every completed grade in order — the attempt's trajectory — carrying the
    // level it graded and whether it was the unaided build of that level. A
    // view can draw the climb with its bands, and a flat tail is the stall an
    // operator otherwise discovers by diffing round logs.
    series: totals.map(total => {
      const mark = grading.findLast(entry => entry.index < total.index) ?? null;
      return { score: total.score, max: total.max, level: mark?.level ?? null,
        unaided: mark ? mark.round === 0 : false };
    }),
  };
}

function summarizeAttempt(plan: CompiledCampaignPlan, attempt: CampaignAttemptState,
  campaignDirectory: string, repairs: number, { includeLog = false }: {
    includeLog?: boolean;
  } = {}) {
  const inspected = inspectCampaignAttempt(plan, attempt, campaignDirectory);
  const execution = inspected.execution;
  let executionDirectory = null;
  let log = '';
  let logUpdatedAt = null;
  if (execution) {
    executionDirectory = contained(campaignDirectory, execution.output, 'campaign execution');
    const logPath = join(executionDirectory, 'process.stdout.log');
    log = readTextTail(logPath);
    // When the run last wrote anything. A running attempt whose output has
    // been silent for a long time is wedged in a way no score can show.
    if (existsSync(logPath)) logUpdatedAt = new Date(statSync(logPath).mtimeMs).toISOString();
  }
  const progress = parseRunProgress(log, { repairs, running: attempt.status === 'running',
    status: attempt.status, dependency: plan.definition.mode.id === 'dependency' });
  if (inspected.result?.score) progress.latestScore = inspected.result.score;
  return {
    ...inspected,
    progress,
    logUpdatedAt,
    ...(includeLog ? { log: log.split(/\r?\n/).slice(-160).join('\n') } : {}),
  };
}

export function summarizeCampaign(directory: string, {
  includeLogs = false,
  includePackage = false,
  includeAttempts = true,
  controllerActive = null,
}: {
  includeLogs?: boolean;
  includePackage?: boolean;
  includeAttempts?: boolean;
  controllerActive?: ControllerActive | null;
} = {}) {
  const { plan, state } = readCampaignState(directory, { requireCurrentInputs: false });
  let attempts = includeAttempts
    ? state.attempts.map(attempt => summarizeAttempt(plan, attempt, directory,
      repairBudgetLimit(plan.definition.repair, {
        features: plan.featureCatalog?.definition.nodes.length ?? 1,
        depths: plan.definition.levels.length,
      }), { includeLog: includeLogs }))
    : [];
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

export interface UnreadableDashboardCampaign {
  key: string;
  id: string;
  title: string;
  status: 'unreadable';
  error: string;
  attempts: [];
}

export type DashboardCampaign = ReturnType<typeof summarizeCampaign>;
export type DashboardCampaignSummary = DashboardCampaign | UnreadableDashboardCampaign;

const overviewCampaignCache = new Map<string, {
  fingerprint: string;
  campaign: DashboardCampaign;
}>();

function summarizeOverviewCampaign(directory: string, includeAttempts: boolean,
  controllerActive: ControllerActive): DashboardCampaign {
  const fingerprint = [CAMPAIGN_FILE.plan, CAMPAIGN_FILE.state]
    .map(file => {
      const stat = statSync(join(directory, file));
      return `${stat.size}:${stat.mtimeMs}`;
    }).join('|');
  const key = `${includeAttempts ? 'attempts' : 'summary'}:${directory}`;
  const cached = overviewCampaignCache.get(key);
  if (cached?.fingerprint === fingerprint) return cached.campaign;
  const campaign = summarizeCampaign(directory, { includeAttempts, controllerActive });
  if (campaign.summary.running === 0) {
    overviewCampaignCache.set(key, { fingerprint, campaign });
  }
  return campaign;
}

export function discoverCampaigns(campaignsRoot: string, {
  includeLogs = false,
  controllerActive = campaignLockIsActive,
}: { includeLogs?: boolean; controllerActive?: ControllerActive } = {}) {
  if (!existsSync(campaignsRoot)) return [];
  const campaigns: DashboardCampaignSummary[] = [];
  for (const entry of readdirSync(campaignsRoot, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const directory = join(campaignsRoot, entry.name);
    if (!existsSync(join(directory, CAMPAIGN_FILE.state))
      || !existsSync(join(directory, CAMPAIGN_FILE.plan))) continue;
    try {
      campaigns.push(includeLogs
        ? summarizeCampaign(directory, { includeLogs, controllerActive })
        : summarizeOverviewCampaign(directory, false, controllerActive));
    } catch (error) {
      campaigns.push({ key: entry.name, id: entry.name, title: entry.name,
        status: 'unreadable', error: errorMessage(error), attempts: [] });
    }
  }
  campaigns.sort((left, right) => String('updatedAt' in right ? right.updatedAt ?? '' : '')
    .localeCompare(String('updatedAt' in left ? left.updatedAt ?? '' : '')));
  if (includeLogs) return campaigns;

  const verdict = campaigns.find(campaign => campaign.status === 'completed'
    && 'facts' in campaign && campaign.facts.grading.status === 'qualified');
  return campaigns.map(campaign => {
    if (campaign.status !== 'running' && campaign !== verdict) return campaign;
    try {
      return summarizeOverviewCampaign(join(campaignsRoot, campaign.key), true, controllerActive);
    } catch (error) {
      const unreadable: UnreadableDashboardCampaign = {
        key: campaign.key,
        id: campaign.id,
        title: campaign.title,
        status: 'unreadable',
        error: errorMessage(error),
        attempts: [],
      };
      return unreadable;
    }
  });
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
  repairBudget?: number;
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
        repairBudget: repairBudgetLimit(plan.definition.repair, {
          features: plan.featureCatalog?.definition.nodes.length ?? 1,
          depths: plan.definition.levels.length,
        }),
        sha256: plan.contentSha256, file: entry.name });
    } catch (error) {
      plans.push({ id: entry.name.slice(0, -5), title: entry.name, state: 'invalid',
        error: errorMessage(error), file: entry.name });
    }
  }
  return plans.sort((left, right) => left.title.localeCompare(right.title));
}

export function readJsonLines(path: string): unknown[] {
  if (!existsSync(path)) return [];
  const lines = readFileSync(path, 'utf8').split(/\r?\n/);
  const last = lines.findLastIndex(line => line.trim() !== '');
  const events: unknown[] = [];
  for (let index = 0; index <= last; index += 1) {
    const line = lines[index];
    if (!line?.trim()) continue;
    try { events.push(JSON.parse(line)); }
    catch {
      if (index === last) break;
      throw new Error(`dashboard operation feed line ${index + 1} is invalid JSON`);
    }
  }
  return events;
}
