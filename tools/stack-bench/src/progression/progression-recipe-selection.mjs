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
  const allowedSpecifications = new Set(scope.flatMap(item => item.promptModules));
  const visitModule = (ref, expectedType, chain = []) => {
    const module = modules.get(ref);
    if (!module) throw new Error(`progression references module ${ref} outside the selected recipe`);
    if (module.moduleType !== expectedType) {
      throw new Error(`progression ${expectedType} dependency ${ref} has type ${module.moduleType}`);
    }
    if (chain.includes(ref)) {
      throw new Error(`recipe module dependency cycle: ${[...chain, ref].join(' -> ')}`);
    }
    for (const requiredRef of module.requiresPacks ?? []) {
      const allowed = expectedType === 'feature' ? allowedFeatures : allowedSpecifications;
      if (!allowed.has(requiredRef)) {
        throw new Error(`progression node ${node.id} requires ${requiredRef} in its node or ancestors`);
      }
      visitModule(requiredRef, expectedType, [...chain, ref]);
    }
  };
  for (const ref of node.featureRefs) visitModule(ref, 'feature');
  for (const ref of node.promptModules) visitModule(ref, 'specification');
  const checks = new Map(binding.release.checkCatalog.map(check => [check.stableKey, check]));
  const validateExpected = (ref, chain = []) => {
    const module = modules.get(ref);
    if (!module || module.moduleType !== 'specification') {
      throw new Error(`progression expected specification ${ref} is invalid`);
    }
    if (chain.includes(ref)) {
      throw new Error(`recipe module dependency cycle: ${[...chain, ref].join(' -> ')}`);
    }
    for (const requiredRef of module.requiresPacks ?? []) {
      validateExpected(requiredRef, [...chain, ref]);
    }
  };
  for (const selected of node.gradingChecks) {
    const check = checks.get(selected.id);
    if (!check) continue;
    const owner = binding.release.components.packs.find(module => module.id === check.packId);
    if (owner?.moduleType === 'specification') {
      validateExpected(`${owner.id}@${owner.version}`);
    }
  }
}

function resolveSelections(binding, definition, promptNodeIds, gradingNodeIds, gradingChecks) {
  if (!binding?.release || !binding?.plan) {
    throw new Error('progression recipe selection requires a resolved recipe binding');
  }
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
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
  const promptSpecifications = checkedRefs(refsFor(promptNodeIds, 'promptModules'));
  if (promptFeatures.some(item => item.module.moduleType !== 'feature')) {
    throw new Error('progression featureRefs must reference feature modules');
  }
  if (promptSpecifications.some(item => item.module.moduleType !== 'specification')) {
    throw new Error('progression promptModules must reference specification modules');
  }

  const gradingFeatures = checkedRefs(refsFor(gradingNodeIds, 'featureRefs'));
  const gradingRequested = checkedRefs(refsFor(gradingNodeIds, 'promptModules'));
  if (gradingFeatures.some(item => item.module.moduleType !== 'feature')) {
    throw new Error('progression featureRefs must reference feature modules');
  }
  if (gradingRequested.some(item => item.module.moduleType !== 'specification')) {
    throw new Error('progression promptModules must reference specification modules');
  }
  const requestedRefs = new Set(gradingRequested.map(item => item.ref));
  const checkKeys = gradingChecks.map(check => check.id);
  const expectedSpecifications = new Set();
  const addExpectedSpecification = (ref, chain = []) => {
    const module = modules.get(ref);
    if (!module || module.moduleType !== 'specification') {
      throw new Error(`progression expected specification ${ref} is invalid`);
    }
    if (chain.includes(ref)) {
      throw new Error(`recipe module dependency cycle: ${[...chain, ref].join(' -> ')}`);
    }
    if (!requestedRefs.has(ref)) expectedSpecifications.add(ref);
    for (const requiredRef of module.requiresPacks ?? []) {
      addExpectedSpecification(requiredRef, [...chain, ref]);
    }
  };
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
      addExpectedSpecification(ownerRef);
    }
  }

  const agent = createBoundRecipeTaskRequest(binding, {
    featureIds: promptFeatures.map(item => item.id),
    requestedSpecifications: promptSpecifications.map(item => item.ref),
    checkKeys: [],
    dependencyExpansion: 'exact',
  });
  const grader = createBoundRecipeTaskRequest(binding, {
    featureIds: gradingFeatures.map(item => item.id),
    requestedSpecifications: gradingRequested.map(item => item.ref),
    expectedSpecifications: [...expectedSpecifications].sort(),
    checkKeys,
    dependencyExpansion: 'exact',
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
  return { action, ...resolveSelections(binding, state.definition, action.prompt.nodeIds,
    action.grading.nodeIds, action.grading.checks) };
}

export function resolveProgressionRecipeLevelSelection(binding, input, level) {
  const { definition } = validateProgressionInput(input);
  const promptNodeIds = definition.nodes.filter(node => node.level === level)
    .map(node => node.id);
  if (promptNodeIds.length === 0) {
    throw new Error(`progression has no nodes at level ${level}`);
  }
  const gradingNodes = definition.nodes.filter(node => node.level <= level);
  return resolveSelections(binding, definition, promptNodeIds,
    gradingNodes.map(node => node.id), gradingNodes.flatMap(node =>
      node.gradingChecks.map(check => ({ ...check, nodeId: node.id }))));
}

export function validateProgressionRecipeBindings(input, bindings) {
  const { definition } = validateProgressionInput(input);
  const byLevel = new Map(bindings.map(binding => [binding.level, binding.binding ?? binding]));
  for (const level of [...new Set(definition.nodes.map(node => node.level))]) {
    const binding = byLevel.get(level);
    if (!binding) throw new Error(`progression has no recipe binding for level ${level}`);
    const levelNodes = definition.nodes.filter(node => node.level === level);
    for (const node of levelNodes) {
      resolveSelections(binding, definition, [node.id], [node.id],
        node.gradingChecks.map(check => ({ ...check, nodeId: node.id })));
    }
    resolveProgressionRecipeLevelSelection(binding, input, level);
  }
  return input;
}
