import { createAgentVisibleTaskRequest, createBoundRecipeTaskRequest }
  from '../composition/recipe-selection.mjs';
import { progressionEngine } from './progression-engine.mjs';
import { validateProgressionInput } from './progression-definition.mjs';

function exactRef(value) {
  const split = value.lastIndexOf('@');
  return { id: value.slice(0, split), version: value.slice(split + 1), ref: value };
}

function unique(values) {
  return [...new Set(values)].sort();
}

function dependencyClosure(modules, roots, expectedType, { allowed = null, label = 'progression' } = {}) {
  const found = new Set();
  const visit = (ref, chain = []) => {
    const module = modules.get(ref);
    if (!module) throw new Error(`${label} references module ${ref} outside the selected recipe`);
    if (module.moduleType !== expectedType) {
      throw new Error(`${label} ${expectedType} dependency ${ref} has type ${module.moduleType}`);
    }
    if (chain.includes(ref)) {
      throw new Error(`recipe module dependency cycle: ${[...chain, ref].join(' -> ')}`);
    }
    if (allowed && !allowed.has(ref)) {
      throw new Error(`${label} requires ${ref} in its node or ancestors`);
    }
    found.add(ref);
    for (const requiredRef of module.requiresPacks ?? []) visit(requiredRef, [...chain, ref]);
  };
  for (const ref of roots) visit(ref);
  return found;
}

function validateNodeModuleDependencies(binding, definition, node) {
  const nodes = new Map(definition.nodes.map(item => [item.id, item]));
  const modules = new Map(binding.release.components.packs
    .map(module => [`${module.id}@${module.version}`, module]));
  const ancestorIds = new Set();
  const visitNode = nodeId => {
    for (const parentId of nodes.get(nodeId).dependencies) {
      if (ancestorIds.has(parentId)) continue;
      ancestorIds.add(parentId);
      visitNode(parentId);
    }
  };
  visitNode(node.id);
  const scope = [node, ...[...ancestorIds].map(nodeId => nodes.get(nodeId))];
  const allowedFeatures = new Set(scope.flatMap(item => item.featureRefs));
  const allowedFeatureIds = new Set([...allowedFeatures].map(ref => exactRef(ref).id));
  const promptModules = scope.flatMap(item => item.promptModules).map(ref => {
    const module = modules.get(ref);
    if (!module) throw new Error(`progression references module ${ref} outside the selected recipe`);
    return { ref, module };
  });
  const allowedSpecifications = new Set(promptModules
    .filter(item => item.module.moduleType === 'specification').map(item => item.ref));
  const nodeSpecifications = node.promptModules.filter(ref =>
    modules.get(ref)?.moduleType === 'specification');
  const unownedPromptFeature = node.promptModules.find(ref =>
    modules.get(ref)?.moduleType === 'feature' && !node.featureRefs.includes(ref));
  if (unownedPromptFeature) {
    throw new Error(`progression node ${node.id} prompt feature ${unownedPromptFeature} is not selected`);
  }
  dependencyClosure(modules, node.featureRefs, 'feature', {
    allowed: allowedFeatures, label: `progression node ${node.id}`,
  });
  dependencyClosure(modules, nodeSpecifications, 'specification', {
    allowed: allowedSpecifications, label: `progression node ${node.id}`,
  });
  const checks = new Map(binding.release.checkCatalog.map(check => [check.stableKey, check]));
  for (const selected of node.gradingChecks) {
    const check = checks.get(selected.id);
    if (!check) continue;
    for (const requiredId of check.requiresFeatures ?? []) {
      if (!allowedFeatureIds.has(requiredId)) {
        throw new Error(`progression node ${node.id} check ${selected.id} requires feature `
          + `${requiredId} outside the node and its ancestors`);
      }
    }
    const owner = binding.release.components.packs.find(module => module.id === check.packId);
    if (owner?.moduleType === 'specification') {
      dependencyClosure(modules, [`${owner.id}@${owner.version}`], 'specification', {
        label: 'progression expected specification',
      });
    }
  }
}

function resolveSelections(binding, definition, promptNodeIds, gradingNodeIds, gradingChecks,
  taskMode) {
  if (!binding?.release || !binding?.plan) {
    throw new Error('progression recipe selection requires a resolved recipe binding');
  }
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
  const selectedTaskMode = binding.plan.recipe.task.mode === 'action' ? taskMode : undefined;
  for (const nodeId of promptNodeIds) {
    validateNodeModuleDependencies(binding, definition, nodes.get(nodeId));
  }
  const modules = new Map(binding.release.components.packs
    .map(module => [`${module.id}@${module.version}`, module]));
  const checkCatalog = new Map(binding.release.checkCatalog
    .map(check => [check.stableKey, check]));
  const refsFor = (nodeIds, field) => unique(nodeIds.flatMap(nodeId => nodes.get(nodeId)[field]));
  const checkedRefs = refs => refs.map(ref => {
    const module = modules.get(ref);
    if (!module) throw new Error(`progression references module ${ref} outside the selected recipe`);
    return { ...exactRef(ref), module };
  });
  const promptFeatures = checkedRefs(refsFor(promptNodeIds, 'featureRefs'));
  const selectedPromptModules = checkedRefs(refsFor(promptNodeIds, 'promptModules'));
  const promptSpecifications = selectedPromptModules
    .filter(item => item.module.moduleType === 'specification');
  if (promptFeatures.some(item => item.module.moduleType !== 'feature')) {
    throw new Error('progression featureRefs must reference feature modules');
  }
  const promptFeatureRefs = new Set(promptFeatures.map(item => item.ref));
  if (selectedPromptModules.some(item => item.module.moduleType === 'feature'
    && !promptFeatureRefs.has(item.ref))) {
    throw new Error('progression prompt feature must also be selected by featureRefs');
  }

  const gradingFeatures = checkedRefs(refsFor(gradingNodeIds, 'featureRefs'));
  const selectedGradingModules = checkedRefs(refsFor(gradingNodeIds, 'promptModules'));
  const gradingRequested = selectedGradingModules
    .filter(item => item.module.moduleType === 'specification');
  if (gradingFeatures.some(item => item.module.moduleType !== 'feature')) {
    throw new Error('progression featureRefs must reference feature modules');
  }
  const gradingFeatureRefs = new Set(gradingFeatures.map(item => item.ref));
  if (selectedGradingModules.some(item => item.module.moduleType === 'feature'
    && !gradingFeatureRefs.has(item.ref))) {
    throw new Error('progression prompt feature must also be selected by featureRefs');
  }
  const requestedRefs = new Set(gradingRequested.map(item => item.ref));
  const checkKeys = gradingChecks.map(check => check.id);
  const expectedSpecifications = new Set();
  const requiredGradingFeatureIds = new Set(gradingFeatures.map(item => item.id));
  for (const selected of gradingChecks) {
    const check = checkCatalog.get(selected.id);
    if (!check) throw new Error(`progression references check ${selected.id} outside the selected recipe`);
    if (check.points !== selected.points) {
      throw new Error(`progression points for ${selected.id} differ from the selected recipe`);
    }
    const owner = binding.release.components.packs.find(module => module.id === check.packId);
    if (!owner) throw new Error(`progression check ${selected.id} has no recipe module owner`);
    const ownerRef = `${owner.id}@${owner.version}`;
    if (owner.moduleType === 'feature'
      && !gradingFeatures.some(item => item.id === owner.id)) {
      throw new Error(`progression check ${selected.id} belongs to an unselected feature`);
    }
    if (owner.moduleType === 'specification' && !requestedRefs.has(ownerRef)) {
      for (const featureId of check.requiresFeatures ?? []) requiredGradingFeatureIds.add(featureId);
      for (const ref of dependencyClosure(modules, [ownerRef], 'specification', {
        label: 'progression expected specification',
      })) {
        if (!requestedRefs.has(ref)) expectedSpecifications.add(ref);
      }
    }
  }

  const agent = createBoundRecipeTaskRequest(binding, {
    featureIds: promptFeatures.map(item => item.id),
    requestedSpecifications: promptSpecifications.map(item => item.ref),
    checkKeys: [],
    dependencyExpansion: 'exact',
    taskMode: selectedTaskMode,
  });
  const grader = createBoundRecipeTaskRequest(binding, {
    featureIds: [...requiredGradingFeatureIds].sort(),
    requestedSpecifications: gradingRequested.map(item => item.ref),
    expectedSpecifications: [...expectedSpecifications].sort(),
    checkKeys,
    dependencyExpansion: 'exact',
    taskMode: selectedTaskMode,
  });
  return {
    agent: { request: createAgentVisibleTaskRequest(binding, agent),
      selection: agent.selection, task: agent.task,
      selectionSha256: agent.selection.sha256, taskSha256: agent.task.sha256 },
    grader: { request: grader.request, selection: grader.selection, task: grader.task,
      selectionSha256: grader.selection.sha256,
      checkKeys: grader.selection.scoredChecks.map(check => check.stableKey) },
  };
}

export function resolveProgressionRecipeAction(binding, state) {
  const action = progressionEngine.nextAction(state);
  if (action.type === 'terminal') return { action };
  const taskMode = action.type === 'build' && action.level === 1 && state.attempts.length === 0
    ? 'fresh' : 'upgrade';
  return { action, ...resolveSelections(binding, state.definition, action.prompt.nodeIds,
    action.grading.nodeIds, action.grading.checks, taskMode) };
}

export function resolveProgressionRecipeLevelSelection(binding, input, level,
  { cumulative = true } = {}) {
  const { definition } = validateProgressionInput(input);
  const promptNodeIds = definition.nodes.filter(node => node.level === level)
    .map(node => node.id);
  if (promptNodeIds.length === 0) {
    throw new Error(`progression has no nodes at level ${level}`);
  }
  const gradingNodes = definition.nodes.filter(node => cumulative
    ? node.level <= level : node.level === level);
  return resolveSelections(binding, definition, promptNodeIds,
    gradingNodes.map(node => node.id), gradingNodes.flatMap(node =>
      node.gradingChecks.map(check => ({ ...check, nodeId: node.id }))),
    level === 1 ? 'fresh' : 'upgrade');
}

export function validateProgressionRecipeBindings(input, bindings, { levels = null } = {}) {
  const { definition } = validateProgressionInput(input);
  const byLevel = new Map(bindings.map(binding => [binding.level, binding.binding ?? binding]));
  const selectedLevels = levels ?? [...new Set(definition.nodes.map(node => node.level))];
  for (const level of selectedLevels) {
    const binding = byLevel.get(level);
    if (!binding) throw new Error(`progression has no recipe binding for level ${level}`);
    const levelNodes = definition.nodes.filter(node => node.level === level);
    for (const node of levelNodes) {
      resolveSelections(binding, definition, [node.id], [node.id],
        node.gradingChecks.map(check => ({ ...check, nodeId: node.id })),
        node.level === 1 ? 'fresh' : 'upgrade');
      if (node.level === 1 && binding.plan.recipe.task.mode === 'action') {
        const repair = resolveSelections(binding, definition, [node.id], [node.id],
          node.gradingChecks.map(check => ({ ...check, nodeId: node.id })), 'upgrade');
        for (const reference of node.featureRefs) {
          const featureId = exactRef(reference).id;
          for (const [kind, ids] of [['requirements', repair.agent.task.requirementIds],
            ['contracts', repair.agent.task.contractIds]]) {
            if (!binding.plan.recipe.task[kind].some(fragment => ids.includes(fragment.id)
              && fragment.owners.includes(featureId))) {
              throw new Error(`progression root ${node.id} feature ${featureId} has no upgrade ${kind}`);
            }
          }
        }
      }
    }
    resolveProgressionRecipeLevelSelection(binding, input, level);
  }
  return input;
}
