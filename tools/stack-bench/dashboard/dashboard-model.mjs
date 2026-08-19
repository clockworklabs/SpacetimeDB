import { closeSync, existsSync, fstatSync, openSync, readFileSync, readSync, readdirSync,
  lstatSync, realpathSync, statSync,
} from 'node:fs';
import { basename, dirname, extname, join, relative, resolve, sep } from 'node:path';

import { readArtifact, readArtifactPayload } from '../src/evidence/artifacts.mjs';
import { compileCampaignFile, validateCompiledCampaignPlan } from '../src/campaigns/campaign-compiler.mjs';
import { campaignLockIsActive } from '../src/campaigns/campaign-lock.mjs';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.mjs';
import { readCampaignState, validateCampaignState } from '../src/campaigns/campaign-scheduler.mjs';
import { redactCredentials } from '../src/evidence/diagnostic-sanitizer.mjs';

const MAX_LOG_BYTES = 96 * 1024;
const MAX_PUBLIC_TEXT_BYTES = 8 * 1024 * 1024;
const MAX_ARTIFACTS_PER_EXECUTION = 512;
const IMAGE_TYPES = new Map([
  ['.png', 'image/png'], ['.jpg', 'image/jpeg'], ['.jpeg', 'image/jpeg'], ['.webp', 'image/webp'],
]);
const CAMPAIGN_ARTIFACT = /^(?:plan\.json|state\.json|report\/report\.(?:html|json))$/;
const EXECUTION_ARTIFACT = /^(?:run\.json|preflight\.json|recovery\.json|process\.(?:stdout|stderr)\.log|backend\.log|level-l\d+-checkpoint\.json|(?:first-build-l\d+-grading|l\d+-fix\d+-grading|grading)\/(?:bundle\.json|contract-lint\.json|actions\.json|grading-[^/]+\.json|failure-media\/[^/]+\.(?:png|jpe?g|webp)))$/i;

function contained(root, path, label) {
  const absoluteRoot = resolve(root);
  const absolute = resolve(absoluteRoot, path);
  const rel = relative(absoluteRoot, absolute);
  if (rel === '..' || rel.startsWith(`..${sep}`) || rel === '') {
    throw new Error(`${label} is outside the configured dashboard root`);
  }
  return absolute;
}

function readTextTail(path, limit = MAX_LOG_BYTES) {
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

function artifactId(relativePath) {
  return Buffer.from(relativePath, 'utf8').toString('base64url');
}

function artifactLabel(path) {
  const name = basename(path);
  if (path === 'plan.json') return 'Frozen plan';
  if (path === 'state.json') return 'Campaign state';
  if (path === 'report/report.html') return 'Campaign report';
  if (path === 'report/report.json') return 'Report data';
  if (name === 'run.json') return 'Run result';
  if (name === 'preflight.json') return 'Preflight result';
  if (name === 'recovery.json') return 'Recovery record';
  if (name === 'process.stdout.log') return 'Run output';
  if (name === 'process.stderr.log') return 'Run errors';
  if (name === 'backend.log') return 'Backend output';
  if (name === 'bundle.json') return `${basename(dirname(path))} bundle`;
  if (name === 'actions.json') return `${basename(dirname(path))} actions`;
  if (name === 'contract-lint.json') return `${basename(dirname(path))} contract check`;
  return name.replace(/[-_]/g, ' ');
}

function artifactMetadata(campaignDirectory, path) {
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

function rejectSymlinkPath(root, path) {
  const rel = relative(resolve(root), resolve(path));
  let current = resolve(root);
  for (const segment of rel.split(sep)) {
    current = join(current, segment);
    if (lstatSync(current).isSymbolicLink()) {
      throw new Error('campaign artifact path contains a symbolic link');
    }
  }
}

function walkPublicExecutionArtifacts(campaignDirectory, executionDirectory) {
  const found = [];
  let truncated = false;
  const visit = directory => {
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
          ? /^(?:first-build-l\d+-grading|l\d+-fix\d+-grading|grading)$/i.test(entry.name)
          : /(?:^|\/)(?:first-build-l\d+-grading|l\d+-fix\d+-grading|grading)$/i.test(directoryRelative)
            && entry.name === 'failure-media';
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

function campaignPackage(campaignDirectory, attempts) {
  const campaign = ['plan.json', 'state.json', 'report/report.html', 'report/report.json']
    .filter(path => existsSync(join(campaignDirectory, path)))
    .map(path => artifactMetadata(campaignDirectory, path));
  const executions = [];
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

export function resolveCampaignArtifact(resultsRoot, key, id) {
  if (!/^[a-z0-9][a-z0-9.-]*$/.test(key)) throw new Error('campaign key is invalid');
  let path;
  try { path = Buffer.from(id, 'base64url').toString('utf8'); }
  catch { throw new Error('campaign artifact id is invalid'); }
  if (!path || artifactId(path) !== id || path.includes('\\') || path.startsWith('/')) {
    throw new Error('campaign artifact id is invalid');
  }
  const executionMatch = path.match(/^attempts\/([^/]+)\/(execution-\d+)\/(.+)$/);
  const allowed = CAMPAIGN_ARTIFACT.test(path)
    || (executionMatch && EXECUTION_ARTIFACT.test(executionMatch[3]));
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

export function readCampaignArtifactBody(artifact) {
  if (artifact.kind === 'visual') return readFileSync(artifact.absolute);
  if (artifact.size > MAX_PUBLIC_TEXT_BYTES) throw new Error('campaign artifact is too large to view');
  return Buffer.from(redactCredentials(readFileSync(artifact.absolute, 'utf8')));
}

function matches(text, pattern) {
  return [...text.matchAll(pattern)].map(match => ({ ...match, index: match.index ?? 0 }));
}

export function parseRunProgress(log, { fixRounds = 0, running = true, status = null } = {}) {
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
  };
}

function readRun(path) {
  if (!existsSync(path)) return null;
  try {
    const run = readArtifactPayload(path, { expectedKind: 'benchmark_run' });
    return {
      outcome: run.outcome?.kind ?? 'ungraded',
      score: run.totals ? { score: run.totals.score, max: run.totals.max } : null,
      costUsd: run.totals?.costUsd ?? null,
      durationSec: run.totals?.durationSec ?? null,
      cleanup: run.backendLease?.state ?? null,
      levels: (run.levels ?? []).map(level => ({
        level: level.level,
        firstScore: level.firstBuild ? { score: level.firstBuild.score, max: level.firstBuild.max } : null,
        finalScore: level.graded ? { score: level.score, max: level.max } : null,
        roundsUsed: level.repair?.roundsUsed ?? 0,
        repairStatus: level.repair?.status ?? null,
        outcome: level.outcome?.kind ?? null,
        // Level records carry spend as it is incurred, while run.totals only
        // appears once an attempt finishes. Reporting both lets a view show
        // what a still-running attempt has already cost.
        costUsd: level.buildCostUsd == null && level.fixCostUsd == null
          ? null : (level.buildCostUsd ?? 0) + (level.fixCostUsd ?? 0),
        failures: (level.missed ?? level.firstBuild?.missed ?? []).map(item =>
          typeof item === 'string' ? item : item?.stableKey ?? item?.description ?? 'Failed check'),
      })),
    };
  } catch (error) {
    return { unreadable: error.message };
  }
}

function readDashboardCampaignState(directory) {
  const statePath = join(directory, 'state.json');
  const rawState = readArtifact(statePath, { expectedKind: 'campaign_state' }).payload;
  if (rawState.schemaVersion !== 1) {
    const plan = validateCompiledCampaignPlan(
      readArtifact(join(directory, 'plan.json'), { expectedKind: 'campaign_plan' }).payload,
      { requireCurrentInputs: false });
    const state = validateCampaignState(rawState);
    if (state.campaignId !== plan.id || state.campaignSha256 !== plan.contentSha256
      || state.maxParallel !== plan.summary.parallelism
      || canonicalDefinitionJson(state.attempts.map(attempt => attempt.plan))
        !== canonicalDefinitionJson(plan.attempts)) {
      throw new Error('stored campaign state does not match its compiled plan');
    }
    return { plan, state };
  }

  // Schema 1 was the active campaign format when the dashboard was introduced.
  // Keep this read-only projection here rather than weakening the scheduler's
  // current schema-2 writer/validator. It can display an already-running run,
  // but no dashboard or CLI command will write the older shape.
  const plan = readArtifact(join(directory, 'plan.json'), { expectedKind: 'campaign_plan' }).payload;
  if (!plan || typeof plan !== 'object' || !Array.isArray(plan.attempts)
    || typeof plan.id !== 'string' || !/^[a-f0-9]{64}$/.test(plan.contentSha256 ?? '')
    || rawState.campaignId !== plan.id || rawState.campaignSha256 !== plan.contentSha256
    || !Array.isArray(rawState.attempts) || rawState.attempts.length !== plan.attempts.length) {
    throw new Error('campaign schema-1 state does not match its compiled plan');
  }
  for (const [index, attempt] of rawState.attempts.entries()) {
    if (!attempt || typeof attempt !== 'object' || attempt.plan?.id !== plan.attempts[index]?.id
      || !['pending', 'running', 'completed', 'invalid'].includes(attempt.status)
      || !Array.isArray(attempt.executions)) {
      throw new Error(`campaign schema-1 attempt ${index + 1} is invalid`);
    }
  }
  return { plan, state: { ...rawState, maxParallel: plan.summary?.parallelism ?? 1 } };
}

function summarizeAttempt(attempt, campaignDirectory, fixRounds, { includeLog = false } = {}) {
  const execution = attempt.executions.at(-1) ?? null;
  let executionDirectory = null;
  let log = '';
  let run = null;
  if (execution) {
    executionDirectory = contained(campaignDirectory, execution.output, 'campaign execution');
    log = readTextTail(join(executionDirectory, 'process.stdout.log'));
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
    result: run,
    ...(includeLog ? { log: log.split(/\r?\n/).slice(-160).join('\n') } : {}),
  };
}

export function summarizeCampaign(directory, {
  includeLogs = false,
  includePackage = false,
  controllerActive = null,
} = {}) {
  const { plan, state } = readDashboardCampaignState(directory);
  let attempts = state.attempts.map(attempt => summarizeAttempt(attempt, directory,
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
    title: plan.title,
    state: plan.state,
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
    attempts,
    ...(includePackage ? { package: campaignPackage(directory, state.attempts) } : {}),
  };
}

export function discoverCampaigns(campaignsRoot, {
  includeLogs = false,
  controllerActive = campaignLockIsActive,
} = {}) {
  if (!existsSync(campaignsRoot)) return [];
  const campaigns = [];
  for (const entry of readdirSync(campaignsRoot, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const directory = join(campaignsRoot, entry.name);
    if (!existsSync(join(directory, 'state.json')) || !existsSync(join(directory, 'plan.json'))) continue;
    try {
      campaigns.push(summarizeCampaign(directory, { includeLogs, controllerActive }));
    } catch (error) {
      campaigns.push({ key: entry.name, id: entry.name, title: entry.name,
        status: 'unreadable', error: error.message, attempts: [] });
    }
  }
  return campaigns.sort((left, right) => String(right.updatedAt ?? '')
    .localeCompare(String(left.updatedAt ?? '')));
}

export function discoverPlans(plansRoot) {
  if (!existsSync(plansRoot)) return [];
  const plans = [];
  for (const entry of readdirSync(plansRoot, { withFileTypes: true })) {
    if (!entry.isFile() || !entry.name.endsWith('.json')) continue;
    const path = join(plansRoot, entry.name);
    try {
      const plan = compileCampaignFile(path);
      plans.push({ id: plan.id, version: plan.version, title: plan.title, state: plan.state,
        track: plan.definition.track, levels: plan.definition.levels,
        stacks: plan.stacks.map(stack => stack.id), attempts: plan.summary.attempts,
        parallelism: plan.summary.parallelism, budgets: plan.definition.budgets,
        sha256: plan.contentSha256, file: entry.name });
    } catch (error) {
      plans.push({ id: entry.name.slice(0, -5), title: entry.name, state: 'invalid',
        error: error.message, file: entry.name });
    }
  }
  return plans.sort((left, right) => left.title.localeCompare(right.title));
}

export function readDashboardOverview({
  resultsRoot,
  plansRoot,
  operations = [],
  controllerActive = campaignLockIsActive,
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

export function campaignDetail(resultsRoot, key, { controllerActive = campaignLockIsActive } = {}) {
  if (!/^[a-z0-9][a-z0-9.-]*$/.test(key)) throw new Error('campaign key is invalid');
  return summarizeCampaign(contained(join(resolve(resultsRoot), 'campaigns'), key, 'campaign'),
    { includeLogs: true, includePackage: true, controllerActive });
}

export function readJsonLines(path) {
  if (!existsSync(path)) return [];
  return readFileSync(path, 'utf8').split(/\r?\n/).filter(Boolean).map((line, index) => {
    try { return JSON.parse(line); }
    catch { throw new Error(`dashboard operation feed line ${index + 1} is invalid JSON`); }
  });
}
