// Stack testability: every selected check is resolved against each selected
// stack before a run starts, so a check the stack cannot measure fails
// compilation instead of stalling a paid attempt as inconclusive evidence.

import { ACTION_REGISTRY } from '../actions/action-catalog.js';
import type { CompiledRecipePlan } from '../composition/composition-compiler.js';
import type { CompiledStep } from '../composition/definition-compiler.js';
import type { NamedAction, TrackAction } from '../composition/tracks.js';
import type { StackGradingSupport } from '../stacks/stack-adapter-contract.js';

export interface TestabilityStack {
  readonly id: string;
  readonly grading: StackGradingSupport;
}

export interface StackTestabilityProblem {
  readonly stack: string;
  readonly check: string;
  readonly action: string;
  readonly reason: string;
}

export interface StackTestabilityInput {
  readonly plan: CompiledRecipePlan;
  readonly checkKeys: readonly string[];
  readonly trackActions: readonly TrackAction[];
  readonly stacks: readonly TestabilityStack[];
}

function* eachStep(steps: readonly CompiledStep[]): Generator<CompiledStep> {
  for (const step of steps) {
    yield step;
    for (const branch of step.branches ?? []) yield* eachStep(branch);
  }
}

// The steps a check runs: its feature's setup and the criterion's own steps. A
// selected feature is split into one group per graded criterion, so the
// group is the one that carries this criterion.
function stepsForCheck(plan: CompiledRecipePlan, key: string): CompiledStep[] {
  const check = plan.checks.find(item => item.stableKey === key);
  if (!check) throw new Error(`selected check ${key} is not in the recipe`);
  for (const group of plan.execution.flatMap(execution => execution.checkGroups)) {
    if (group.source !== check.source || group.packId !== check.packId
      || group.checkGroupId !== check.checkGroupId || group.feature.id !== check.featureId) continue;
    const criterion = group.feature.criteria.find(item => item.id === check.criterionId);
    if (criterion) return [...group.feature.setup, ...criterion.steps];
  }
  throw new Error(`selected check ${key} has no scenario steps`);
}

// Convex also supports one confirmed native mutation with an exact argument
// replacement for the same actor. Other unbound replays still need HTTP capture.
function readsCapturedHttpWrite(step: CompiledStep, transport: StackGradingSupport['transport']): boolean {
  switch (step.do) {
    case 'forgeWrite':
    case 'expectForgeryRejected':
    case 'replayConcurrently':
      return true;
    case 'replayAs':
      return step.namedAction === undefined && !(transport === 'convex' && step.actor === step.from
        && step.swap !== undefined && step.match === (step.swap as { find?: unknown }).find);
    default:
      return false;
  }
}

// The application action a step issues: its inline binding, or the track's
// action of that name. `undefined` means the step issues none.
function applicationAction(step: CompiledStep, trackActions: readonly TrackAction[]):
  NamedAction | null | undefined {
  if (step.namedAction) return step.namedAction as NamedAction;
  if (step.do !== 'callAction' && step.do !== 'callConcurrently') return undefined;
  return trackActions.find(action => action.id === step.action) ?? null;
}

export function resolveStackTestability(input: StackTestabilityInput): StackTestabilityProblem[] {
  const problems = new Map<string, StackTestabilityProblem>();
  const report = (problem: StackTestabilityProblem): void => {
    problems.set([problem.stack, problem.check, problem.action, problem.reason].join('\u0000'), problem);
  };
  for (const check of input.checkKeys) {
    const steps = [...eachStep(stepsForCheck(input.plan, check))];
    for (const stack of input.stacks) {
      const provided = new Set<string>(stack.grading.capabilities);
      for (const step of steps) {
        const action = step.do;
        for (const capability of ACTION_REGISTRY.get(action).capabilities) {
          if (provided.has(capability)) continue;
          report({ stack: stack.id, check, action,
            reason: `needs the ${capability} capability, which ${stack.id} does not provide` });
        }
        if (readsCapturedHttpWrite(step, stack.grading.transport) && stack.grading.transport !== 'http') {
          report({ stack: stack.id, check, action,
            reason: `re-issues a captured HTTP write, and ${stack.id} issues writes as reducer calls; give the step a named action` });
        }
        const named = applicationAction(step, input.trackActions);
        if (named === undefined) continue;
        if (named === null) {
          report({ stack: stack.id, check, action,
            reason: `names no application action ${JSON.stringify(step.action ?? '')}` });
          continue;
        }
        const id = named.id ?? step.action ?? '';
        if (stack.grading.transport === 'reducer' && !named.reducer) {
          report({ stack: stack.id, check, action,
            reason: `application action ${id} declares no reducer for ${stack.id}` });
        }
        if (stack.grading.transport === 'http' && !named.path) {
          report({ stack: stack.id, check, action,
            reason: `application action ${id} declares no route for ${stack.id}` });
        }
      }
    }
  }
  return [...problems.values()];
}

export function describeStackTestabilityProblems(problems: readonly StackTestabilityProblem[]): string[] {
  return problems.map(problem =>
    `${problem.stack} cannot measure ${problem.check}: ${problem.action} ${problem.reason}`);
}
