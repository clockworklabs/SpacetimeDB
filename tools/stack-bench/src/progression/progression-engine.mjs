import { dependencyModePolicy } from './dependency-mode.mjs';

export function createProgressionEngine(policies) {
  if (!Array.isArray(policies) || policies.length === 0) {
    throw new Error('progression engine requires at least one policy');
  }
  const byId = new Map();
  for (const policy of policies) {
    if (!policy || typeof policy.id !== 'string' || !policy.id) {
      throw new Error('progression policy requires an id');
    }
    if (byId.has(policy.id)) throw new Error(`duplicate progression policy ${policy.id}`);
    for (const method of [
      'compile', 'initialize', 'activeNodes', 'promptSelection', 'gradingSelection',
      'recordResult', 'grantStrikes', 'resume', 'nextAction', 'score',
    ]) {
      if (typeof policy[method] !== 'function') {
        throw new Error(`progression policy ${policy.id} requires ${method}()`);
      }
    }
    byId.set(policy.id, policy);
  }
  const definitionPolicy = definition => {
    const policy = byId.get(definition?.policy);
    if (!policy) throw new Error(`unknown progression policy ${JSON.stringify(definition?.policy)}`);
    return policy;
  };
  const statePolicy = state => {
    const policy = byId.get(state?.policy);
    if (!policy) throw new Error(`unknown progression state policy ${JSON.stringify(state?.policy)}`);
    return policy;
  };
  return Object.freeze({
    compile: definition => definitionPolicy(definition).compile(definition),
    initialize: definition => definitionPolicy(definition).initialize(definition),
    activeNodes: state => statePolicy(state).activeNodes(state),
    promptSelection: state => statePolicy(state).promptSelection(state),
    gradingSelection: state => statePolicy(state).gradingSelection(state),
    recordResult: (state, result) => statePolicy(state).recordResult(state, result),
    grantStrikes: (state, grant) => statePolicy(state).grantStrikes(state, grant),
    resume: state => statePolicy(state).resume(state),
    nextAction: state => statePolicy(state).nextAction(state),
    score: state => statePolicy(state).score(state),
  });
}

export const progressionEngine = createProgressionEngine([dependencyModePolicy]);
