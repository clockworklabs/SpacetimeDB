import { progressionEngine } from './progression-engine.mjs';
import { validateProgressionInput } from './progression-definition.mjs';
import { acquireProgressionStateLock, progressionStateExists, readProgressionState,
  releaseProgressionStateLock, writeProgressionState } from './progression-state.mjs';

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);

export async function runProgressionMode({ definition = null, state: inputState = null,
  execute, onState = null, engine = progressionEngine } = {}) {
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
    const result = await execute(structuredClone(action), structuredClone(state));
    if (!object(result)) throw new Error('progression runner execute() returned an invalid result');
    state = engine.recordResult(state, result);
    if (onState) await onState(structuredClone(state));
    if (result.outcome === 'inconclusive') {
      return { status: 'paused', outcome: { kind: result.category,
        reason: result.reason }, state, score: engine.score(state) };
    }
  }
}

export async function runPersistedProgressionMode({ progression, owner, statePath, execute,
  onState = null, engine = progressionEngine } = {}) {
  progression = validateProgressionInput(progression);
  if (typeof statePath !== 'string' || !statePath) {
    throw new Error('persisted progression runner requires statePath');
  }
  const lock = acquireProgressionStateLock(statePath, progression, owner);
  try {
    let state;
    if (progressionStateExists(statePath)) {
      state = readProgressionState(statePath, { progression, owner,
        requireCurrentEngine: true }).state;
    } else {
      state = engine.initialize(progression.definition);
      writeProgressionState(statePath, { progression, owner, state });
    }
    return await runProgressionMode({ state, execute, engine,
      onState: async next => {
        writeProgressionState(statePath, { progression, owner, state: next });
        if (onState) await onState(structuredClone(next));
      } });
  } finally {
    releaseProgressionStateLock(lock);
  }
}
