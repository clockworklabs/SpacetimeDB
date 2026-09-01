import type { ArtifactIdentities } from './artifacts.js';
import type { OutcomeBundle, RunOutcome } from './outcomes.js';
import type { SessionMetricsInput, SessionMetricsSummary } from './session-metrics.js';
import type { PublicBackendLease } from '../runtime/backend-lease.js';
import type { LevelCheckpoint } from '../runtime/source-checkpoint.js';
import type { ValidatedAgentResult } from '../agents/agent-result-contract.js';

type UnknownRecord = Record<string, unknown>;

export interface GradeBundleCheck {
  stableKey: string;
  points: number;
  executionId?: string;
  packId?: string;
}

export interface GradeBundleCriterion {
  id?: string;
  stableKey?: string;
  points?: number;
  evidence?: unknown;
}

export interface GradeBundleFeature {
  id?: string;
  name?: string;
  criteria?: GradeBundleCriterion[];
  cleanupEvidence?: { status?: string };
}

export interface GradeBundleSuite {
  total?: number;
  max?: number;
  pass?: boolean;
  features?: GradeBundleFeature[];
  results?: Array<{ id: string; status: string; detail?: string }>;
  cleanupEvidence?: { status?: string };
}

export interface GradeBundleSelection {
  sha256?: string;
  checks?: GradeBundleCheck[];
  scoredChecks?: GradeBundleCheck[];
  observedChecks?: GradeBundleCheck[];
  attemptedChecks?: string[];
  reportedChecks?: string[];
  notRun?: unknown[];
}

export interface GradeBundleTotals {
  score?: number | null;
  max?: number | null;
  dirty?: boolean;
  contractPass?: boolean | null;
  regression?: { score?: number | null; max?: number | null } | null;
}

export interface GradeBundlePayload extends OutcomeBundle {
  suites?: Record<string, GradeBundleSuite>;
  totals?: GradeBundleTotals;
  selection?: GradeBundleSelection | null;
  code?: unknown;
  error?: string;
  outcome?: RunOutcome;
  source?: { sha256: string };
}

export interface RunSessionRecord {
  round?: number;
  sessionId: string | null;
  costUsd: number;
  durationMs: number;
  usage: SessionMetricsInput['usage'];
  costReceipts: unknown[];
  costComplete: boolean;
  providerThrottle: SessionMetricsInput['providerThrottle'];
  tokens: number | null;
  outputTokens: number | null;
  turns: number | null;
  promptBytes: number | null;
  thinking: SessionMetricsInput['thinking'];
  transcript: unknown;
  provenance: unknown;
  providerMetadata: unknown;
}

export interface RunRepairRecord {
  status: string;
  budgetRounds: number;
  roundsUsed: number;
  stopReason: string | null;
  strikeScope?: 'feature';
  nodeStrikes?: Array<{
    nodeId: string;
    initialBudget: number;
    granted: number;
    budget: number;
    used: number;
    remaining: number;
    exhaustionReason: string | null;
  }>;
}

export interface RunLevelRecord {
  level: number;
  graded: boolean;
  score: number | null;
  max: number | null;
  selection: GradeBundleSelection | null;
  outcome: RunOutcome;
  error?: string;
  code?: unknown;
  firstBuild?: UnknownRecord;
  baseline?: UnknownRecord;
  resumedRepair?: UnknownRecord;
  regression?: { score?: number | null; max?: number | null } | null;
  contractPass?: boolean | null;
  buildSession?: RunSessionRecord;
  resumeSession?: RunSessionRecord;
  fixSessions?: RunSessionRecord[];
  buildCostUsd?: number;
  resumeCostUsd?: number;
  fixCostUsd?: number;
  costUsd?: number;
  fixRounds?: number;
  durationMs?: number;
  durationSec?: number;
  sessionTotals?: SessionMetricsSummary;
  repairHistory?: Array<{
    round: number;
    beforeScore: number | null;
    beforeMax: number | null;
    afterScore: number | null;
    afterMax: number | null;
    result: string;
    remainingFailures: string[];
  }>;
  tokens?: number;
  usage?: SessionMetricsSummary['usage'];
  turns?: number;
  promptBytes?: number;
  tokensPerTurn?: number | null;
  thinking?: SessionMetricsSummary['thinking'];
  priorRepairRounds?: number;
  cumulativeFixRounds?: number;
  repair?: RunRepairRecord;
  checkpoint?: LevelCheckpoint | null;
  stalled?: boolean;
  regressed?: boolean;
}

export interface RunTotals {
  score: number;
  max: number;
  costUsd: number | null;
  costComplete: boolean;
  fixRounds: number;
  sessions: number;
  tokens: number;
  outputTokens: number;
  turns: number;
  modelDurationMs: number;
  durationSec: number;
  ungraded: number[];
  priorExecutionCostUsd?: number | null;
  currentExecutionCostUsd?: number;
  cumulativeCostUsd?: number | null;
}

export interface RunProgressionResume {
  priorRunId: string;
  priorRunSha256: string;
  stateSnapshotSha256: string | null;
  action: { type: string; level?: number };
  inheritedLevels: number[];
  priorTotals: RunTotals | null;
}

export interface RunProgressionStatus {
  stateArtifact: string;
  phase: 'active' | 'terminal';
  level: number;
  attempts: number;
  score: unknown;
}

export interface RunContinuation {
  cumulativeRoundsBefore: number;
  cumulativeCostBeforeUsd: number;
  cumulativeDurationBeforeSec: number;
  cumulativeRoundsAfter?: number;
  cumulativeCostAfterUsd?: number | null;
  cumulativeDurationAfterSec?: number;
  resumeSetup?: UnknownRecord | null;
  baseline?: UnknownRecord | null;
}

export interface BenchmarkRunRecord {
  id: string;
  startedAt: string;
  completedAt?: string;
  parentAttemptId: string | null;
  identities: ArtifactIdentities;
  mode: unknown;
  track: string;
  backend: string;
  model: string;
  pricing: unknown;
  guidance: string;
  condition: unknown;
  skills: unknown[];
  runtime: { buildImage: string; url: string };
  selectionRequest: unknown;
  featureCatalog: unknown;
  dependencyPolicy: unknown;
  progressionOwner?: UnknownRecord;
  progressionStatus?: RunProgressionStatus;
  progressionResume?: RunProgressionResume;
  progressionResumeFrom?: unknown;
  backendLease: PublicBackendLease;
  backendDiagnostics?: unknown;
  validation: {
    ladder: {
      policy: string | undefined;
      requestedLevels: number[];
      completedLevels: number[];
      stoppedAfterLevel: number | null;
      blockedLevels: number[];
    };
  };
  levels: RunLevelRecord[];
  continuation?: RunContinuation;
  setup?: UnknownRecord & {
    session?: string;
    isolation?: { imageId?: string | null };
  };
  contaminated?: boolean;
  contamination?: unknown;
  mutationControl?: UnknownRecord & { ok?: boolean; skipped?: boolean; outcome?: RunOutcome | null;
    processError?: string | null; imageId?: string | null };
  outcome?: RunOutcome;
  totals?: RunTotals;
}

export function addCostUsd(...values: Array<number | null | undefined>): number {
  let total = 0;
  for (const value of values) total += value ?? 0;
  return Number(total.toFixed(6));
}

export function runSessionRecord(
  session: ValidatedAgentResult,
  round: number | null = null,
): RunSessionRecord {
  return {
    ...(round === null ? {} : { round }),
    sessionId: session.sessionId ?? null,
    costUsd: session.costUsd,
    durationMs: session.durationMs,
    usage: session.usage ?? null,
    costReceipts: session.costReceipts ?? [],
    costComplete: session.costComplete === true,
    providerThrottle: session.setup?.providerThrottle ?? null,
    tokens: session.tokens ?? null,
    outputTokens: session.outputTokens ?? null,
    turns: session.turns ?? null,
    promptBytes: session.promptBytes ?? null,
    thinking: session.thinking ?? null,
    transcript: session.transcript ?? null,
    provenance: session.provenance ?? null,
    providerMetadata: session.providerMetadata ?? null,
  };
}

interface RunTotalsLevel {
  level: number;
  graded?: boolean;
  score?: number | null;
  max?: number | null;
  buildCostUsd?: number | null;
  resumeCostUsd?: number | null;
  fixCostUsd?: number | null;
  fixRounds?: number | null;
  sessionTotals?: {
    sessions?: number;
    tokens?: number;
    outputTokens?: number;
    turns?: number;
    durationMs?: number;
  };
}

export interface RunTotalsInput {
  levels: RunTotalsLevel[];
  progressionResume?: {
    inheritedLevels: number[];
    priorTotals: Pick<RunTotals, 'costUsd' | 'costComplete'> | null;
  };
  totals?: BenchmarkRunRecord['totals'];
}

export function finalizeRunTotals(
  run: RunTotalsInput,
  started: number,
  { now = Date.now(), costComplete = true }: { now?: number; costComplete?: boolean } = {},
) {
  const inherited = new Set(run.progressionResume?.inheritedLevels ?? []);
  const currentLevels = run.levels.filter(level => !inherited.has(level.level));
  const currentExecutionCostUsd = addCostUsd(currentLevels.reduce((n, level) => n
    + (level.buildCostUsd ?? level.resumeCostUsd ?? 0)
    + (level.fixCostUsd ?? 0), 0));
  const priorExecutionCostUsd = run.progressionResume?.priorTotals?.costUsd ?? null;
  const cumulativeCostUsd = run.progressionResume
    ? (typeof priorExecutionCostUsd === 'number'
      ? addCostUsd(priorExecutionCostUsd, currentExecutionCostUsd) : null)
    : currentExecutionCostUsd;
  run.totals = {
    score: run.levels.reduce((n, level) => n + (level.score ?? 0), 0),
    max: run.levels.reduce((n, level) => n + (level.max ?? 0), 0),
    costUsd: cumulativeCostUsd,
    costComplete: costComplete && (!run.progressionResume
      || (typeof priorExecutionCostUsd === 'number'
        && run.progressionResume.priorTotals?.costComplete !== false)),
    ...(run.progressionResume ? { priorExecutionCostUsd, currentExecutionCostUsd,
      cumulativeCostUsd } : {}),
    fixRounds: run.levels.reduce((n, level) => n + (level.fixRounds ?? 0), 0),
    sessions: run.levels.reduce((n, level) => n + (level.sessionTotals?.sessions ?? 0), 0),
    tokens: run.levels.reduce((n, level) => n + (level.sessionTotals?.tokens ?? 0), 0),
    outputTokens: run.levels.reduce((n, level) => n + (level.sessionTotals?.outputTokens ?? 0), 0),
    turns: run.levels.reduce((n, level) => n + (level.sessionTotals?.turns ?? 0), 0),
    modelDurationMs: run.levels.reduce((n, level) => n
      + (level.sessionTotals?.durationMs ?? 0), 0),
    durationSec: Math.round((now - started) / 1000),
    ungraded: run.levels.filter(level => !level.graded).map(level => level.level),
  };
  return run.totals;
}
