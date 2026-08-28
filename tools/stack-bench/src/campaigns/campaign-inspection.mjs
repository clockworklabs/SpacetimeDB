import { existsSync } from 'node:fs';
import { join } from 'node:path';

import { readCampaignState } from './campaign-scheduler.mjs';
import { progressionEngine } from '../progression/progression-engine.mjs';
import { readProgressionState } from '../progression/progression-state.mjs';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../progression/progression-definition.mjs';

function progressionOwner(plan, attempt) {
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

export function dependencyProgress(plan, attempt, executionDirectory) {
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
      const checks = Object.values(node.checks);
      return {
        id,
        title: definition.title,
        level: definition.level,
        // The questline and the dependency ids let a view draw the graph the
        // engine walks, instead of re-deriving structure from prose.
        questline: definition.questline,
        dependencies: [...definition.dependencies],
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
    return {
      phase: state.phase,
      level: state.level,
      attempts: {
        total: state.attempts.length,
        level: state.attempts.filter(item => item.level === state.level).length,
        maxRemaining: strikes?.maxRemaining ?? 0,
        features: strikes?.nodes ?? [],
      },
      work: {
        current: nodes.filter(node => node.status === 'active'),
        passed: nodes.filter(node => node.status === 'passed'),
        failed: nodes.filter(node => ['exhausted', 'regressed'].includes(node.status)
          || node.checks.failed > 0),
        locked: nodes.filter(node => ['locked', 'blocked'].includes(node.status)),
      },
      questlines: state.definition.questlines.map(questline => ({
        id: questline.id, title: questline.title,
        // the declared, ordered membership — a view must never re-derive it
        nodes: [...questline.nodes] })),
      nodes,
      score: progressionEngine.score(state),
      evidence: state.attempts.map((item, index) => ({
        attempt: index + 1,
        level: item.level,
        outcome: item.outcome,
        runId: item.runId ?? null,
        sourceSha256: item.sourceSha256 ?? null,
        selectionSha256: item.selectionSha256 ?? null,
      })),
      snapshotSha256: stored.snapshotSha256,
    };
  } catch (error) {
    return { unreadable: error.message };
  }
}

export function inspectCampaignSummary(directory) {
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
