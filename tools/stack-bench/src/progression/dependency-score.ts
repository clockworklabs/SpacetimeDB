import type { CompiledProgressionDefinition } from './progression-definition.js';
import type {
  ProgressionNodeState,
  ProgressionState,
  ProgressionTerminalOutcome,
} from './progression-state.js';
import { dependencyNodeState } from './dependency-state.js';
import { checkCompletion, type CheckCompletion, type CheckStatus } from '../evidence/check-completion.js';

export const INCONCLUSIVE_CATEGORIES = [
  'provider_failure',
  'harness_failure',
  'interrupted',
  'inconclusive_evidence',
] as const;

export type InconclusiveCategory = typeof INCONCLUSIVE_CATEGORIES[number];

interface PointTotals {
  passedPoints: number;
  failedPoints: number;
  blockedPoints: number;
  gradedPoints: number;
  ungradedPoints: number;
  availablePoints: number;
}

export interface DependencyScore {
  completion: CheckCompletion;
  status: 'final' | 'provisional';
  terminalOutcome: ProgressionTerminalOutcome | null;
  attempts: {
    total: number;
    inconclusive: number;
    inconclusiveByCategory: Record<InconclusiveCategory, number>;
    conclusive: number;
  };
  questlines: Array<PointTotals & {
    completion: CheckCompletion;
    id: string;
    title: string;
    percentage: number | null;
    provisionalPercentage: null;
  }>;
  questlineAveragePercentage: number | null;
  uniqueChecks: PointTotals & { percentage: number | null; provisionalPercentage: null };
  nodes: Array<PointTotals & {
    completion: CheckCompletion;
    id: string;
    status: ProgressionNodeState['status'];
    blockedBy: string[];
  }>;
}

interface ScoringState extends ProgressionState {
  definition: CompiledProgressionDefinition;
}

function nodePoints(state: ScoringState, nodeId: string): PointTotals {
  const node = state.definition.nodes.find(candidate => candidate.id === nodeId);
  if (!node) throw new Error(`unknown dependency node ${nodeId}`);
  const nodeState = dependencyNodeState(state, nodeId);
  const checks = nodeState.checks;
  const blocked = nodeState.status === 'blocked';
  const passedPoints = node.gradingChecks.reduce((total, check) =>
    total + (!blocked && checks[check.id] === 'pass' ? check.points : 0), 0);
  const failedPoints = node.gradingChecks.reduce((total, check) =>
    total + (!blocked && checks[check.id] === 'fail' ? check.points : 0), 0);
  const blockedPoints = blocked
    ? node.gradingChecks.reduce((total, check) => total + check.points, 0) : 0;
  const gradedPoints = passedPoints + failedPoints;
  const availablePoints = node.gradingChecks.reduce((total, check) => total + check.points, 0);
  return { passedPoints, failedPoints, gradedPoints, blockedPoints,
    ungradedPoints: availablePoints - gradedPoints - blockedPoints,
    availablePoints };
}

function addPoints(total: PointTotals, points: PointTotals): PointTotals {
  total.passedPoints += points.passedPoints;
  total.failedPoints += points.failedPoints;
  total.blockedPoints += points.blockedPoints;
  total.gradedPoints += points.gradedPoints;
  total.ungradedPoints += points.ungradedPoints;
  total.availablePoints += points.availablePoints;
  return total;
}

const emptyPoints = (): PointTotals => ({
  passedPoints: 0,
  failedPoints: 0,
  blockedPoints: 0,
  gradedPoints: 0,
  ungradedPoints: 0,
  availablePoints: 0,
});

const percentage = ({ passedPoints, availablePoints }: PointTotals): number =>
  (passedPoints / availablePoints) * 100;

export interface DependencyCompletionBreakdown {
  featureCompletion: Pick<CheckCompletion, 'selected' | 'passed' | 'rate'>;
  checkCategories: Record<'feature' | 'production' | 'interface' | 'unknown', CheckCompletion>;
}

function dependencyOutcomes(state: ScoringState): Map<string, CheckStatus> {
  return new Map<string, CheckStatus>(state.definition.nodes.flatMap(node => {
    const current = dependencyNodeState(state, node.id);
    return node.gradingChecks.map(check => [check.id, current.status === 'blocked' ? 'blocked'
      : current.checks[check.id] === 'pass' ? 'passed'
      : current.checks[check.id] === 'fail' ? 'failed' : 'unmeasured'] as const);
  }));
}

/** Read-only breakdown of the pinned target; blocked checks never receive credit. */
export function dependencyCompletionBreakdown(state: ScoringState): DependencyCompletionBreakdown {
  const checks = state.definition.nodes.flatMap(node => node.gradingChecks);
  const outcomes = dependencyOutcomes(state);
  const selected = state.definition.nodes.length;
  const passed = state.definition.nodes.filter(node =>
    dependencyNodeState(state, node.id).status === 'passed').length;
  return {
    featureCompletion: { selected, passed, rate: selected ? passed / selected : null },
    checkCategories: {
      feature: checkCompletion(checks.filter(check => check.category === 'feature'), outcomes),
      production: checkCompletion(checks.filter(check => check.category === 'production'), outcomes),
      interface: checkCompletion(checks.filter(check => check.category === 'interface'), outcomes),
      unknown: checkCompletion(checks.filter(check => check.category === undefined), outcomes),
    },
  };
}

export function scoreDependencyState(state: ScoringState): DependencyScore {
  const final = state.phase === 'terminal';
  const outcomes = dependencyOutcomes(state);
  const questlines = state.definition.questlines.map(questline => {
    const points = questline.nodes.reduce((total, nodeId) =>
      addPoints(total, nodePoints(state, nodeId)), emptyPoints());
    return { id: questline.id, title: questline.title, ...points,
      completion: checkCompletion(state.definition.nodes.filter(node => questline.nodes.includes(node.id))
        .flatMap(node => node.gradingChecks), outcomes),
      percentage: final ? percentage(points) : null,
      provisionalPercentage: null };
  });
  const uniqueChecks = state.definition.nodes.reduce((total, node) =>
    addPoints(total, nodePoints(state, node.id)), emptyPoints());
  const nodes = state.definition.nodes.map(node => {
    const nodeState = dependencyNodeState(state, node.id);
    return {
      id: node.id,
      completion: checkCompletion(node.gradingChecks, outcomes),
      status: nodeState.status,
      blockedBy: nodeState.status === 'blocked'
        ? node.dependencies.filter(parentId => {
          const status = dependencyNodeState(state, parentId).status;
          return status === 'failed' || status === 'blocked';
        })
        : [],
      ...nodePoints(state, node.id),
    };
  });
  const inconclusiveAttempts = state.attempts.filter(attempt =>
    attempt.outcome === 'inconclusive').length;
  const inconclusiveByCategory = Object.fromEntries(INCONCLUSIVE_CATEGORIES
    .map(category => [category, state.attempts.filter(attempt =>
      attempt.category === category).length])) as Record<InconclusiveCategory, number>;
  return {
    completion: checkCompletion(state.definition.nodes.flatMap(node => node.gradingChecks), outcomes),
    status: final ? 'final' : 'provisional',
    terminalOutcome: final && state.terminalOutcome ? { ...state.terminalOutcome } : null,
    attempts: { total: state.attempts.length, inconclusive: inconclusiveAttempts,
      inconclusiveByCategory,
      conclusive: state.attempts.length - inconclusiveAttempts },
    questlines,
    questlineAveragePercentage: final
      ? questlines.reduce((total, questline) => total + (questline.percentage ?? 0), 0)
        / questlines.length
      : null,
    uniqueChecks: { ...uniqueChecks,
      percentage: final ? percentage(uniqueChecks) : null,
      provisionalPercentage: null },
    nodes,
  };
}
