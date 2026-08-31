import { existsSync } from 'node:fs';
import { join } from 'node:path';

import { readCampaignState } from './campaign-scheduler.js';
import { progressionEngine } from '../progression/progression-engine.js';
import { readProgressionState } from '../progression/progression-state.js';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../progression/progression-definition.js';
import type { DependencyEvent, DependencyState } from '../progression/dependency-mode.js';
import type { CampaignAttemptPlan, CompiledCampaignPlan } from './campaign-compiler.js';
import type { DependencyPromptSelection } from '../progression/dependency-mode.js';

export interface DependencyProgressNode {
  id: string;
  title: string;
  depth: number;
  questline: string;
  dependencies: string[];
  blockedBy: string[];
  status: string;
  strikes: { budget: number; used: number; remaining: number };
  exhaustionReason: unknown;
  checks: { passed: number; failed: number; total: number };
}

export interface DependencyProgressScore {
  status?: string;
  questlineAveragePercentage?: number | null;
  uniqueChecks?: { percentage?: number | null; availablePoints?: number; passedPoints?: number };
  questlines?: Array<{
    id: string;
    percentage?: number | null;
    availablePoints?: number;
    passedPoints?: number;
  }>;
}

export interface DependencyProgressQuestline {
  id: string;
  title: string;
  nodes: string[];
}

export interface DependencyProgressEvidence {
  attempt: number;
  depth: number;
  outcome: string;
  runId: string | null;
  sourceSha256: string | null;
  selectionSha256: string | null;
}

export interface DependencyProgress {
  phase: string;
  activeDepths: number[];
  attempts: {
    total: number;
    maxRemaining: number;
    features: Array<Record<string, unknown>>;
  };
  work: {
    current: DependencyProgressNode[];
    working: DependencyProgressNode[];
    passed: DependencyProgressNode[];
    failed: DependencyProgressNode[];
    blocked: DependencyProgressNode[];
    waiting: DependencyProgressNode[];
  };
  nodes: DependencyProgressNode[];
  questlines?: DependencyProgressQuestline[];
  score?: DependencyProgressScore;
  history?: {
    firstTryPercentage: number;
    repairAttempts: number;
  };
  evidence?: DependencyProgressEvidence[];
  unreadable?: string;
  [key: string]: unknown;
}

function dependencyHistory(state: DependencyState): NonNullable<DependencyProgress['history']> {
  const firstOutcomes = new Map<string, string>();
  let replay = progressionEngine.initialize(state.definition);
  let repairAttempts = 0;
  for (const event of state.events as DependencyEvent[]) {
    if (event.type === 'strikes-granted') {
      replay = progressionEngine.grantStrikes(replay, event.grant);
      continue;
    }
    const action = progressionEngine.nextAction(replay);
    if (event.result.outcome === 'conclusive') {
      if (action.type === 'repair') repairAttempts += 1;
      for (const node of event.result.nodes) {
        for (const check of node.checks) {
          const key = `${node.id}\u0000${check.id}`;
          if (!firstOutcomes.has(key)) firstOutcomes.set(key, check.outcome);
        }
      }
    }
    replay = progressionEngine.recordResult(replay, event.result);
  }
  const byNode = new Map(state.definition.nodes.map(node => [node.id, node]));
  const questlinePercentages = state.definition.questlines.map(questline => {
    let passed = 0;
    let available = 0;
    for (const nodeId of questline.nodes) {
      const node = byNode.get(nodeId);
      if (!node) continue;
      for (const check of node.gradingChecks) {
        available += check.points;
        if (firstOutcomes.get(`${nodeId}\u0000${check.id}`) === 'pass') passed += check.points;
      }
    }
    return available ? (passed / available) * 100 : 0;
  });
  return {
    firstTryPercentage: questlinePercentages.length
      ? questlinePercentages.reduce((sum, value) => sum + value, 0)
        / questlinePercentages.length
      : 0,
    repairAttempts,
  };
}

function progressionOwner(plan: CompiledCampaignPlan, attempt: CampaignAttemptPlan) {
  return {
    schemaVersion: 1,
    campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
    attempt: {
      id: attempt.id,
      track: plan.definition.track,
      stack: attempt.stack,
      agentAdapter: attempt.agentAdapter,
      model: attempt.model,
      conditionSha256: attempt.condition.sha256,
    },
    workspace: { appDirectory: 'source' },
  };
}

export function dependencyProgress(plan: CompiledCampaignPlan, attempt: CampaignAttemptPlan,
  executionDirectory: string | null): DependencyProgress | null {
  if (attempt.mode?.id !== 'dependency' || !plan.featureCatalog
    || !plan.dependencyPolicy || !executionDirectory) return null;
  const statePath = join(executionDirectory, 'progression-state.json');
  if (!existsSync(statePath)) return null;
  try {
    const stored = readProgressionState(statePath, {
      progression: compileProgressionInput(dependencyRuntimeDefinition(
        plan.featureCatalog, plan.dependencyPolicy)),
      featureCatalogIdentity: plan.featureCatalog.identity,
      dependencyPolicyIdentity: plan.dependencyPolicy.identity,
      owner: progressionOwner(plan, attempt),
    });
    const state = stored.state;
    const definitions = new Map(state.definition.nodes.map(node => [node.id, node]));
    const nodes = Object.entries(state.nodes).map(([id, node]) => {
      const definition = definitions.get(id);
      if (!definition) throw new Error(`progression state has unknown node ${id}`);
      const checks = Object.values(node.checks);
      return {
        id,
        title: definition.title,
        depth: definition.level,
        // The questline and the dependency ids let a view draw the graph the
        // engine walks, instead of re-deriving structure from prose.
        questline: definition.questline,
        dependencies: [...definition.dependencies],
        blockedBy: definition.dependencies.filter(parentId => {
          const parent = state.nodes[parentId];
          return parent?.status === 'failed' || parent?.status === 'blocked';
        }),
        status: node.status,
        strikes: { ...node.strikes, remaining: node.strikes.budget - node.strikes.used },
        exhaustionReason: node.exhaustionReason,
        checks: {
          passed: checks.filter(value => value === 'pass').length,
          failed: checks.filter(value => value === 'fail').length,
          total: checks.length,
        },
      };
    });
    const action = progressionEngine.nextAction(state);
    const strikes = action.type === 'terminal' ? null : action.strikes;
    const prompt = action.type === 'terminal' ? null : action.prompt as DependencyPromptSelection;
    const currentIds = new Set(prompt?.nodeIds ?? []);
    const activeDepths = [...new Set(nodes.filter(node => currentIds.has(node.id))
      .map(node => node.depth))].sort((left, right) => left - right);
    return {
      phase: state.phase,
      activeDepths,
      attempts: {
        total: state.attempts.length,
        maxRemaining: strikes?.maxRemaining ?? 0,
        features: strikes?.nodes ?? [],
      },
      work: {
        current: nodes.filter(node => currentIds.has(node.id)),
        working: nodes.filter(node => node.status === 'working'),
        passed: nodes.filter(node => node.status === 'passed'),
        failed: nodes.filter(node => node.status === 'failed'),
        blocked: nodes.filter(node => node.status === 'blocked'),
        waiting: nodes.filter(node => node.status === 'locked'),
      },
      questlines: state.definition.questlines.map(questline => ({
        id: questline.id, title: questline.title,
        // the declared, ordered membership — a view must never re-derive it
        nodes: [...questline.nodes] })),
      nodes,
      score: progressionEngine.score(state),
      history: dependencyHistory(state as DependencyState),
      evidence: state.attempts.map((item, index) => ({
        attempt: index + 1,
        depth: item.level,
        outcome: item.outcome,
        runId: item.runId ?? null,
        sourceSha256: item.sourceSha256 ?? null,
        selectionSha256: item.selectionSha256 ?? null,
      })),
      snapshotSha256: stored.snapshotSha256,
    };
  } catch (error) {
    return { unreadable: error instanceof Error ? error.message : String(error), phase: 'unreadable',
      activeDepths: [], attempts: { total: 0, maxRemaining: 0, features: [] },
      work: { current: [], working: [], passed: [], failed: [], blocked: [], waiting: [] },
      nodes: [] };
  }
}

export function inspectCampaignSummary(directory: string) {
  const { plan, state } = readCampaignState(directory, { requireCurrentInputs: false });
  return {
    id: plan.id,
    version: plan.version,
    sha256: plan.contentSha256,
    mode: plan.definition.mode?.id ?? 'sequential',
    status: state.status,
    attempts: state.attempts.map(attempt => {
      const execution = attempt.executions.at(-1) ?? null;
      const executionDirectory = execution ? join(directory, execution.output) : null;
      return {
        id: attempt.plan.id,
        status: attempt.status,
        executions: attempt.executions.length,
        dependency: dependencyProgress(plan, attempt.plan, executionDirectory),
      };
    }),
  };
}
