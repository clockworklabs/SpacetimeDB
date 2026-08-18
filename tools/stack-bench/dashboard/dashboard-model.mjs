import { closeSync, existsSync, fstatSync, openSync, readFileSync, readSync, readdirSync,
} from 'node:fs';
import { basename, join, relative, resolve, sep } from 'node:path';

import { readArtifact, readArtifactPayload } from '../artifacts.mjs';
import { compileCampaignFile } from '../campaign-compiler.mjs';
import { readCampaignState } from '../campaign-scheduler.mjs';

const MAX_LOG_BYTES = 96 * 1024;
const SECRET = /\b(?:authorization\s*:\s*)?bearer\s+[A-Za-z0-9._~+\/-]+=*|\b(?:api[_-]?key|access[_-]?token|auth[_-]?token|password)\s*[=:]\s*[^\s,;]+|\b(?:sk|key)-[A-Za-z0-9_-]{16,}\b/gi;

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
    return buffer.toString('utf8').replace(SECRET, '[redacted credential]');
  } finally {
    closeSync(descriptor);
  }
}

function matches(text, pattern) {
  return [...text.matchAll(pattern)].map(match => ({ ...match, index: match.index ?? 0 }));
}

export function parseRunProgress(log, { fixRounds = 0, running = true, status = null } = {}) {
  const totals = matches(log, /^\s*TOTAL\b.*?(\d+)\/(\d+)\s*$/gm)
    .map(match => ({ index: match.index, score: Number(match[1]), max: Number(match[2]) }));
  const roundMarkers = matches(log, /^--- fix round (\d+)\/(\d+) ---$/gm)
    .map(match => ({ index: match.index, round: Number(match[1]), budget: Number(match[2]) }));
  const grading = matches(log, /^===\s+[^\n]*?-l(\d+)-(first|fix(\d+))\b.*$/gm)
    .map(match => ({ index: match.index, level: Number(match[1]),
      round: match[3] ? Number(match[3]) : 0 }));
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
  if (rawState.schemaVersion !== 1) return readCampaignState(directory);

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

export function summarizeCampaign(directory, { includeLogs = false } = {}) {
  const { plan, state } = readDashboardCampaignState(directory);
  const attempts = state.attempts.map(attempt => summarizeAttempt(attempt, directory,
    plan.definition.budgets.fixRounds, { includeLog: includeLogs }));
  return {
    key: basename(resolve(directory)),
    id: plan.id,
    version: plan.version,
    title: plan.title,
    state: plan.state,
    status: state.status,
    track: plan.definition.track,
    levels: plan.definition.levels,
    stacks: plan.stacks.map(stack => stack.id),
    repetitions: plan.definition.repetitions,
    maxParallel: state.maxParallel,
    createdAt: state.createdAt,
    updatedAt: state.updatedAt,
    summary: state.summary,
    budgets: plan.definition.budgets,
    attempts,
  };
}

export function discoverCampaigns(campaignsRoot, { includeLogs = false } = {}) {
  if (!existsSync(campaignsRoot)) return [];
  const campaigns = [];
  for (const entry of readdirSync(campaignsRoot, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const directory = join(campaignsRoot, entry.name);
    if (!existsSync(join(directory, 'state.json')) || !existsSync(join(directory, 'plan.json'))) continue;
    try {
      campaigns.push(summarizeCampaign(directory, { includeLogs }));
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

export function readDashboardOverview({ resultsRoot, plansRoot, operations = [] }) {
  const campaignsRoot = join(resolve(resultsRoot), 'campaigns');
  const campaigns = discoverCampaigns(campaignsRoot);
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

export function campaignDetail(resultsRoot, key) {
  if (!/^[a-z0-9][a-z0-9.-]*$/.test(key)) throw new Error('campaign key is invalid');
  return summarizeCampaign(contained(join(resolve(resultsRoot), 'campaigns'), key, 'campaign'),
    { includeLogs: true });
}

export function readJsonLines(path) {
  if (!existsSync(path)) return [];
  return readFileSync(path, 'utf8').split(/\r?\n/).filter(Boolean).map((line, index) => {
    try { return JSON.parse(line); }
    catch { throw new Error(`dashboard operation feed line ${index + 1} is invalid JSON`); }
  });
}
