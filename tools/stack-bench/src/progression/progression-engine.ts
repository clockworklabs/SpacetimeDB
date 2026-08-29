import { dependencyModePolicy } from './dependency-mode.mjs';
import type {
  ProgressionState,
  ProgressionTerminalOutcome,
} from './progression-state.js';

export interface ProgressionTerminalAction extends Record<string, unknown> {
  type: 'terminal';
  outcome: ProgressionTerminalOutcome;
}

export interface ProgressionWorkAction extends Record<string, unknown> {
  type: 'build' | 'repair';
  level: number;
  strikes: {
    scope: 'feature';
    maxRemaining: number;
    nodes: Array<{
      nodeId: string;
      initialBudget: number;
      granted: number;
      budget: number;
      used: number;
      remaining: number;
    }>;
  };
  prompt: unknown;
  grading: unknown;
}

export type ProgressionAction = ProgressionTerminalAction | ProgressionWorkAction;

export interface ProgressionPolicy<
  TDefinition = unknown,
  TState = unknown,
  TAction = unknown,
  TScore = unknown,
> {
  id: string;
  compile(definition: unknown): TDefinition;
  initialize(definition: unknown): TState;
  activeNodes(state: TState): unknown;
  promptSelection(state: TState): unknown;
  gradingSelection(state: TState): unknown;
  recordResult(state: TState, result: unknown): TState;
  grantStrikes(state: TState, grant: unknown): TState;
  resume(state: unknown): TState;
  nextAction(state: TState): TAction;
  score(state: TState): TScore;
}

export interface ProgressionEngine<
  TDefinition = unknown,
  TState = unknown,
  TAction = unknown,
  TScore = unknown,
> extends Omit<ProgressionPolicy<TDefinition, TState, TAction, TScore>, 'id'> {}

const POLICY_METHODS = Object.freeze([
  'compile', 'initialize', 'activeNodes', 'promptSelection', 'gradingSelection',
  'recordResult', 'grantStrikes', 'resume', 'nextAction', 'score',
] as const satisfies ReadonlyArray<keyof Omit<ProgressionPolicy, 'id'>>);

function hasProperties(value: unknown): value is Record<string, unknown> {
  return value !== null && (typeof value === 'object' || typeof value === 'function');
}

function policyId(value: unknown): unknown {
  return hasProperties(value) ? value.policy : undefined;
}

export function createProgressionEngine<TDefinition, TState, TAction, TScore>(
  policies: readonly ProgressionPolicy<TDefinition, TState, TAction, TScore>[],
): Readonly<ProgressionEngine<TDefinition, TState, TAction, TScore>>;
export function createProgressionEngine(policies: unknown): Readonly<ProgressionEngine>;
export function createProgressionEngine(policies: unknown): Readonly<ProgressionEngine> {
  if (!Array.isArray(policies) || policies.length === 0) {
    throw new Error('progression engine requires at least one policy');
  }
  const byId = new Map<string, ProgressionPolicy>();
  for (const candidate of policies) {
    if (!hasProperties(candidate) || typeof candidate.id !== 'string' || !candidate.id) {
      throw new Error('progression policy requires an id');
    }
    const policyId = candidate.id;
    if (byId.has(policyId)) throw new Error(`duplicate progression policy ${policyId}`);
    for (const method of POLICY_METHODS) {
      if (typeof candidate[method] !== 'function') {
        throw new Error(`progression policy ${policyId} requires ${method}()`);
      }
    }
    byId.set(policyId, candidate as unknown as ProgressionPolicy);
  }
  const definitionPolicy = (definition: unknown): ProgressionPolicy => {
    const id = policyId(definition);
    const policy = typeof id === 'string' ? byId.get(id) : undefined;
    if (!policy) throw new Error(`unknown progression policy ${JSON.stringify(id)}`);
    return policy;
  };
  const statePolicy = (state: unknown): ProgressionPolicy => {
    const id = policyId(state);
    const policy = typeof id === 'string' ? byId.get(id) : undefined;
    if (!policy) throw new Error(`unknown progression state policy ${JSON.stringify(id)}`);
    return policy;
  };
  return Object.freeze({
    compile: (definition: unknown) => definitionPolicy(definition).compile(definition),
    initialize: (definition: unknown) => definitionPolicy(definition).initialize(definition),
    activeNodes: (state: unknown) => statePolicy(state).activeNodes(state),
    promptSelection: (state: unknown) => statePolicy(state).promptSelection(state),
    gradingSelection: (state: unknown) => statePolicy(state).gradingSelection(state),
    recordResult: (state: unknown, result: unknown) =>
      statePolicy(state).recordResult(state, result),
    grantStrikes: (state: unknown, grant: unknown) =>
      statePolicy(state).grantStrikes(state, grant),
    resume: (state: unknown) => statePolicy(state).resume(state),
    nextAction: (state: unknown) => statePolicy(state).nextAction(state),
    score: (state: unknown) => statePolicy(state).score(state),
  });
}

export const progressionEngine = createProgressionEngine([dependencyModePolicy]);
