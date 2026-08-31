import type { CompiledProgressionDefinition } from './progression-definition.js';
import type {
  ProgressionNodeState,
  ProgressionState,
  ProgressionTerminalOutcome,
} from './progression-state.js';

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
  testSystemPoints: number;
  gradedPoints: number;
  ungradedPoints: number;
  availablePoints: number;
}

export interface DependencyScore {
  status: 'final' | 'provisional';
  terminalOutcome: ProgressionTerminalOutcome | null;
  attempts: {
    total: number;
    inconclusive: number;
    inconclusiveByCategory: Record<InconclusiveCategory, number>;
    conclusive: number;
  };
  questlines: Array<PointTotals & {
    id: string;
    title: string;
    percentage: number | null;
    provisionalPercentage: null;
  }>;
  questlineAveragePercentage: number | null;
  uniqueChecks: PointTotals & { percentage: number | null; provisionalPercentage: null };
  nodes: Array<PointTotals & {
    id: string;
    status: ProgressionNodeState['status'];
    blockedBy: string[];
  }>;
}

interface ScoringState extends ProgressionState {
  definition: CompiledProgressionDefinition;
}

function getNodeState(state: ScoringState, nodeId: string): ProgressionNodeState {
  const node = state.nodes[nodeId];
  if (!node) throw new Error(`dependency mode state is missing node ${nodeId}`);
  return node;
}

function nodePoints(state: ScoringState, nodeId: string): PointTotals {
  const node = state.definition.nodes.find(candidate => candidate.id === nodeId);
  if (!node) throw new Error(`unknown dependency node ${nodeId}`);
  const nodeState = getNodeState(state, nodeId);
  const checks = nodeState.checks;
  const blocked = nodeState.status === 'blocked';
  const passedPoints = node.gradingChecks.reduce((total, check) =>
    total + (!blocked && checks[check.id] === 'pass' ? check.points : 0), 0);
  const failedPoints = node.gradingChecks.reduce((total, check) =>
    total + (!blocked && checks[check.id] === 'fail' ? check.points : 0), 0);
  const blockedPoints = blocked
    ? node.gradingChecks.reduce((total, check) => total + check.points, 0) : 0;
  const testSystemPoints = node.gradingChecks.reduce((total, check) =>
    total + (!blocked && checks[check.id] === 'test-system' ? check.points : 0), 0);
  const gradedPoints = passedPoints + failedPoints;
  const availablePoints = node.gradingChecks.reduce((total, check) => total + check.points, 0);
  return { passedPoints, failedPoints, gradedPoints,
    blockedPoints, testSystemPoints,
    ungradedPoints: availablePoints - gradedPoints - blockedPoints - testSystemPoints,
    availablePoints };
}

function addPoints(total: PointTotals, points: PointTotals): PointTotals {
  total.passedPoints += points.passedPoints;
  total.failedPoints += points.failedPoints;
  total.blockedPoints += points.blockedPoints;
  total.testSystemPoints += points.testSystemPoints;
  total.gradedPoints += points.gradedPoints;
  total.ungradedPoints += points.ungradedPoints;
  total.availablePoints += points.availablePoints;
  return total;
}

const emptyPoints = (): PointTotals => ({
  passedPoints: 0,
  failedPoints: 0,
  blockedPoints: 0,
  testSystemPoints: 0,
  gradedPoints: 0,
  ungradedPoints: 0,
  availablePoints: 0,
});

const percentage = ({ passedPoints, availablePoints }: PointTotals): number =>
  (passedPoints / availablePoints) * 100;

export function scoreDependencyState(state: ScoringState): DependencyScore {
  const final = state.phase === 'terminal';
  const questlines = state.definition.questlines.map(questline => {
    const points = questline.nodes.reduce((total, nodeId) =>
      addPoints(total, nodePoints(state, nodeId)), emptyPoints());
    return { id: questline.id, title: questline.title, ...points,
      percentage: final ? percentage(points) : null,
      provisionalPercentage: null };
  });
  const uniqueChecks = state.definition.nodes.reduce((total, node) =>
    addPoints(total, nodePoints(state, node.id)), emptyPoints());
  const nodes = state.definition.nodes.map(node => {
    const nodeState = getNodeState(state, node.id);
    return {
      id: node.id,
      status: nodeState.status,
      blockedBy: nodeState.status === 'blocked'
        ? node.dependencies.filter(parentId => {
          const status = getNodeState(state, parentId).status;
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
