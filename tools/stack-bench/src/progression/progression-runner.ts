import { progressionEngine } from './progression-engine.js';
import { validateProgressionInput } from './progression-definition.js';
import {
  acquireProgressionStateLock,
  progressionStateExists,
  readProgressionState,
  releaseProgressionStateLock,
  writeProgressionState,
} from './progression-state.js';

export type ProgressionState = Record<string, unknown>;

export interface ProgressionWorkAction extends Record<string, unknown> {
  type: string;
}

export interface ProgressionTerminalAction {
  type: 'terminal';
  outcome: unknown;
}

export interface ProgressionAttemptResult extends Record<string, unknown> {
  outcome: string;
  category?: unknown;
  reason?: unknown;
}

export interface ProgressionEngine {
  initialize(definition: unknown): ProgressionState;
  resume(state: ProgressionState): ProgressionState;
  nextAction(state: ProgressionState): ProgressionTerminalAction | ProgressionWorkAction;
  recordResult(state: ProgressionState, result: ProgressionAttemptResult): ProgressionState;
  score(state: ProgressionState): unknown;
}

export interface ProgressionRunResult {
  status: 'paused' | 'terminal';
  outcome: unknown;
  state: ProgressionState;
  score: unknown;
}

export type ProgressionExecutor = (
  action: ProgressionWorkAction,
  state: ProgressionState,
) => Promise<ProgressionAttemptResult>;

export interface RunProgressionModeOptions {
  definition?: unknown;
  state?: ProgressionState | null;
  execute: ProgressionExecutor;
  onState?: ((state: ProgressionState) => Promise<void> | void) | null;
  engine?: ProgressionEngine;
}

interface PersistedProgressionInput {
  definition: unknown;
}

export interface RunPersistedProgressionModeOptions {
  progression: unknown;
  featureCatalogIdentity: unknown;
  dependencyPolicyIdentity: unknown;
  owner: unknown;
  statePath: string;
  execute: ProgressionExecutor;
  onState?: ((state: ProgressionState) => Promise<void> | void) | null;
  engine?: ProgressionEngine;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function defaultEngine(): ProgressionEngine {
  return progressionEngine;
}

export async function runProgressionMode({
  definition = null,
  state: inputState = null,
  execute,
  onState = null,
  engine = defaultEngine(),
}: RunProgressionModeOptions): Promise<ProgressionRunResult> {
  if ((definition === null) === (inputState === null)) {
    throw new Error('progression runner requires exactly one definition or resumed state');
  }
  if (typeof execute !== 'function') throw new Error('progression runner requires execute()');
  if (onState !== null && typeof onState !== 'function') {
    throw new Error('progression runner onState must be a function');
  }
  let state = inputState === null ? engine.initialize(definition) : engine.resume(inputState);
  while (true) {
    const action = engine.nextAction(state);
    if (action.type === 'terminal') {
      return { status: 'terminal', outcome: action.outcome, state, score: engine.score(state) };
    }
    const workAction = action as ProgressionWorkAction;
    const result = await execute(structuredClone(workAction), structuredClone(state));
    if (!isRecord(result) || typeof result.outcome !== 'string') {
      throw new Error('progression runner execute() returned an invalid result');
    }
    state = engine.recordResult(state, result as ProgressionAttemptResult);
    if (onState) await onState(structuredClone(state));
    if (result.outcome === 'inconclusive') {
      return {
        status: 'paused',
        outcome: { kind: result.category, reason: result.reason },
        state,
        score: engine.score(state),
      };
    }
  }
}

export async function runPersistedProgressionMode({
  progression: rawProgression,
  featureCatalogIdentity,
  dependencyPolicyIdentity,
  owner,
  statePath,
  execute,
  onState = null,
  engine = defaultEngine(),
}: RunPersistedProgressionModeOptions): Promise<ProgressionRunResult> {
  const progression = validateProgressionInput(rawProgression) as PersistedProgressionInput;
  if (typeof statePath !== 'string' || !statePath) {
    throw new Error('persisted progression runner requires statePath');
  }
  const lock = acquireProgressionStateLock(
    statePath,
    progression,
    featureCatalogIdentity,
    dependencyPolicyIdentity,
    owner,
  );
  try {
    let state: ProgressionState;
    if (progressionStateExists(statePath)) {
      state = readProgressionState(statePath, {
        progression,
        featureCatalogIdentity,
        dependencyPolicyIdentity,
        owner,
        requireCurrentEngine: true,
      }).state;
    } else {
      state = engine.initialize(progression.definition);
      writeProgressionState(statePath, {
        progression,
        featureCatalogIdentity,
        dependencyPolicyIdentity,
        owner,
        state,
      });
    }
    return await runProgressionMode({
      state,
      execute,
      engine,
      onState: async next => {
        writeProgressionState(statePath, {
          progression,
          featureCatalogIdentity,
          dependencyPolicyIdentity,
          owner,
          state: next,
        });
        if (onState) await onState(structuredClone(next));
      },
    });
  } finally {
    releaseProgressionStateLock(lock);
  }
}
