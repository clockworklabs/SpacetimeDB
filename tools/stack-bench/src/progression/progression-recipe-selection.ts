import { createAgentVisibleTaskRequest, createBoundRecipeTaskRequest }
  from '../composition/recipe-selection.js';
import type {
  ComposedRecipeTask,
  ModularRecipeTaskRequestResult,
} from '../composition/recipe-selection.js';
import type {
  RecipeBinding,
  RecipeCheck,
  RecipePackComponent,
} from '../composition/recipe-release.js';
import { progressionEngine } from './progression-engine.js';
import type { ProgressionAction, ProgressionWorkAction } from './progression-engine.js';
import { validateFeatureCatalogInput, validateProgressionInput }
  from './progression-definition.js';
import type {
  CompiledProgressionDefinition,
  CompiledProgressionNode,
  ProgressionInput,
} from './progression-definition.js';
import type { ProgressionState } from './progression-state.js';

interface ExactReference {
  id: string;
  version: string;
  ref: string;
}

interface GradingCheckSelection {
  id: string;
  points: number;
  nodeId?: string;
}

interface RequestedSpecifications {
  expected: string[];
  observed: string[];
  requested: string[];
}

interface ModularRequestSelection extends Record<string, unknown> {
  requested: {
    features: string[];
    specifications: RequestedSpecifications;
    checks: string[];
    dependencyExpansion?: string;
  };
  promptPacks: string[];
}

type ModularTaskRequest = ModularRecipeTaskRequestResult['request'] & {
  recipe: { id: string; version: string; contentSha256: string };
  selection: ModularRequestSelection;
};

type ModularBoundSelection = ModularRecipeTaskRequestResult['selection'] & {
  sha256: string;
  requested: ModularRequestSelection['requested'];
  promptPacks: string[];
  scoredChecks: RecipeCheck[];
};

interface ModularBoundTask {
  request: ModularTaskRequest;
  selection: ModularBoundSelection;
  task: ComposedRecipeTask;
}

export interface ProgressionRecipeSelections {
  agent: {
    request: ModularTaskRequest;
    selection: ModularBoundSelection;
    task: ComposedRecipeTask;
    selectionSha256: string;
    taskSha256: string;
  };
  grader: {
    request: ModularTaskRequest;
    selection: ModularBoundSelection;
    task: ComposedRecipeTask;
    selectionSha256: string;
    checkKeys: string[];
  };
}

export type ProgressionRecipeAction =
  | { action: Extract<ProgressionAction, { type: 'terminal' }> }
  | ({ action: ProgressionWorkAction } & ProgressionRecipeSelections);

interface RecipeWorkAction extends ProgressionWorkAction {
  prompt: { nodeIds: string[] };
  grading: { nodeIds: string[]; checks: GradingCheckSelection[] };
}

interface RecipeBindingAtLevel {
  level: number;
  binding: RecipeBinding;
}

interface LeveledRecipeBinding extends RecipeBinding {
  level: number;
}

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function asModularBoundTask(value: unknown): ModularBoundTask {
  if (!object(value) || !object(value.request) || !object(value.selection)
    || !object(value.task) || typeof value.selection.sha256 !== 'string'
    || !Array.isArray(value.selection.scoredChecks)
    || !Array.isArray(value.selection.promptPacks) || !object(value.selection.requested)) {
    throw new Error('progression recipe selection requires a modular recipe task');
  }
  return value as unknown as ModularBoundTask;
}

function asModularTaskRequest(value: unknown): ModularTaskRequest {
  if (!object(value) || !object(value.selection) || !object(value.selection.requested)
    || !Array.isArray(value.selection.promptPacks)) {
    throw new Error('progression agent request requires a modular recipe task');
  }
  return value as unknown as ModularTaskRequest;
}

function asRecipeWorkAction(action: ProgressionWorkAction): RecipeWorkAction {
  if (!object(action.prompt) || !Array.isArray(action.prompt.nodeIds)
    || !object(action.grading) || !Array.isArray(action.grading.nodeIds)
    || !Array.isArray(action.grading.checks)) {
    throw new Error('progression action has invalid prompt or grading selections');
  }
  return action as RecipeWorkAction;
}

const validateCatalog = (input: unknown): ProgressionInput =>
  object(input) && object(input.definition) && input.definition.kind === 'feature-catalog'
    ? validateFeatureCatalogInput(input) : validateProgressionInput(input);

function exactRef(value: string): ExactReference {
  const split = value.lastIndexOf('@');
  return { id: value.slice(0, split), version: value.slice(split + 1), ref: value };
}

function unique(values: string[]): string[] {
  return [...new Set(values)].sort();
}

function dependencyClosure(modules: Map<string, RecipePackComponent>, roots: string[],
  expectedType: string,
  { allowed = null, label = 'progression' }:
  { allowed?: Set<string> | null; label?: string } = {}): Set<string> {
  const found = new Set<string>();
  const visit = (ref: string, chain: string[] = []): void => {
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

function getNode(nodes: Map<string, CompiledProgressionNode>, nodeId: string): CompiledProgressionNode {
  const node = nodes.get(nodeId);
  if (!node) throw new Error(`progression references unknown node ${nodeId}`);
  return node;
}

function validateNodeModuleDependencies(binding: RecipeBinding,
  definition: CompiledProgressionDefinition, node: CompiledProgressionNode): void {
  const nodes = new Map(definition.nodes.map(item => [item.id, item]));
  const modules = new Map(binding.release.components.packs
    .map(module => [`${module.id}@${module.version}`, module]));
  const ancestorIds = new Set<string>();
  const visitNode = (nodeId: string): void => {
    for (const parentId of getNode(nodes, nodeId).dependencies) {
      if (ancestorIds.has(parentId)) continue;
      ancestorIds.add(parentId);
      visitNode(parentId);
    }
  };
  visitNode(node.id);
  const scope = [node, ...[...ancestorIds].map(nodeId => getNode(nodes, nodeId))];
  const allowedFeatures = new Set(scope.flatMap(item => item.featureRefs));
  const graphFeatureIds = new Set(definition.nodes.flatMap(item => item.featureRefs)
    .map(ref => exactRef(ref).id));
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
      if (!graphFeatureIds.has(requiredId)) {
        throw new Error(`progression node ${node.id} check ${selected.id} requires feature `
          + `${requiredId} outside the feature graph`);
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

function resolveSelections(binding: RecipeBinding, definition: CompiledProgressionDefinition,
  promptNodeIds: string[], gradingNodeIds: string[], gradingChecks: GradingCheckSelection[],
  taskMode: 'fresh' | 'upgrade'): ProgressionRecipeSelections {
  if (!binding?.release || !binding?.plan) {
    throw new Error('progression recipe selection requires a resolved recipe binding');
  }
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
  const selectedTaskMode = binding.plan.recipe.task.mode === 'action' ? taskMode : undefined;
  for (const nodeId of promptNodeIds) {
    validateNodeModuleDependencies(binding, definition, getNode(nodes, nodeId));
  }
  const modules = new Map(binding.release.components.packs
    .map(module => [`${module.id}@${module.version}`, module]));
  const checkCatalog = new Map(binding.release.checkCatalog
    .map(check => [check.stableKey, check]));
  const refsFor = (nodeIds: string[], field: 'featureRefs' | 'promptModules'): string[] =>
    unique(nodeIds.flatMap(nodeId => getNode(nodes, nodeId)[field]));
  const checkedRefs = (refs: string[]): Array<ExactReference & { module: RecipePackComponent }> =>
    refs.map(ref => {
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
  const expectedSpecifications = new Set<string>();
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
    for (const featureId of check.requiresFeatures ?? []) {
      requiredGradingFeatureIds.add(featureId);
    }
    if (owner.moduleType === 'specification' && !requestedRefs.has(ownerRef)) {
      for (const ref of dependencyClosure(modules, [ownerRef], 'specification', {
        label: 'progression expected specification',
      })) {
        if (!requestedRefs.has(ref)) expectedSpecifications.add(ref);
      }
    }
  }

  const agent = asModularBoundTask(createBoundRecipeTaskRequest(binding, {
    featureIds: promptFeatures.map(item => item.id),
    requestedSpecifications: promptSpecifications.map(item => item.ref),
    checkKeys: [],
    dependencyExpansion: 'exact',
    taskMode: selectedTaskMode,
  }));
  const grader = asModularBoundTask(createBoundRecipeTaskRequest(binding, {
    featureIds: [...requiredGradingFeatureIds].sort(),
    requestedSpecifications: gradingRequested.map(item => item.ref),
    expectedSpecifications: [...expectedSpecifications].sort(),
    checkKeys,
    dependencyExpansion: 'exact',
    taskMode: selectedTaskMode,
  }));
  return {
    agent: { request: asModularTaskRequest(createAgentVisibleTaskRequest(binding, agent)),
      selection: agent.selection, task: agent.task,
      selectionSha256: agent.selection.sha256, taskSha256: agent.task.sha256 },
    grader: { request: grader.request, selection: grader.selection, task: grader.task,
      selectionSha256: grader.selection.sha256,
      checkKeys: grader.selection.scoredChecks.map(check => check.stableKey) },
  };
}

export function resolveProgressionRecipeAction(binding: RecipeBinding,
  state: ProgressionState): ProgressionRecipeAction {
  const action = progressionEngine.nextAction(state);
  if (action.type === 'terminal') return { action };
  const workAction = asRecipeWorkAction(action);
  const taskMode = action.type === 'build' && state.attempts.length === 0
    ? 'fresh' : 'upgrade';
  return { action, ...resolveSelections(binding, state.definition, workAction.prompt.nodeIds,
    workAction.grading.nodeIds, workAction.grading.checks, taskMode) };
}

export function resolveProgressionRepairTarget(binding: RecipeBinding,
  state: ProgressionState): ProgressionRecipeSelections['grader'] {
  const action = progressionEngine.nextAction(state);
  if (action.type !== 'repair') throw new Error('progression has no repair target');
  const work = asRecipeWorkAction(action);
  const targetIds = new Set(work.prompt.nodeIds);
  const checks = work.grading.checks.filter(check => check.nodeId
    && targetIds.has(check.nodeId));
  if (checks.length === 0) throw new Error('progression repair target has no checks');
  return resolveSelections(binding, state.definition, work.prompt.nodeIds,
    work.prompt.nodeIds, checks, 'upgrade').grader;
}

export function resolveProgressionRecipeLevelSelection(binding: RecipeBinding, input: unknown,
  level: number, { cumulative = true }: { cumulative?: boolean } = {}): ProgressionRecipeSelections {
  const { definition } = validateCatalog(input);
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

interface DeclaredProgressionLevelScope {
  recipe: { contentSha256: string };
  selection: { sha256: string };
  task: { sha256: string };
}

export function validateProgressionCampaignLevelScope(binding: RecipeBinding,
  progression: ProgressionInput, declared: DeclaredProgressionLevelScope | null | undefined,
  level: number): ProgressionRecipeSelections {
  if (!declared) throw new Error(`study condition does not bind requested L${level}`);
  const derived = resolveProgressionRecipeLevelSelection(binding, progression, level);
  if (declared.recipe.contentSha256 !== derived.grader.request.recipe.contentSha256
    || declared.selection.sha256 !== derived.grader.selection.sha256
    || declared.task.sha256 !== derived.grader.task.sha256) {
    throw new Error(`dependency campaign graph-derived scope changed before L${level}`);
  }
  return derived;
}

export function validateProgressionRecipeBindings<T>(input: T,
  bindings: Array<RecipeBindingAtLevel | LeveledRecipeBinding>,
  { levels = null }: { levels?: number[] | null } = {}): T {
  const { definition } = validateCatalog(input);
  const byLevel = new Map(bindings.map(item =>
    [item.level, 'binding' in item ? item.binding : item]));
  const selectedLevels = levels ?? [...new Set(definition.nodes.map(node => node.level))];
  for (const level of selectedLevels) {
    const binding = byLevel.get(level);
    if (!binding) throw new Error(`progression has no recipe binding for level ${level}`);
    const levelNodes = definition.nodes.filter(node => node.level === level);
    for (const node of levelNodes) {
      try {
        resolveSelections(binding, definition, [node.id], [node.id],
          node.gradingChecks.map(check => ({ ...check, nodeId: node.id })),
          node.level === 1 ? 'fresh' : 'upgrade');
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`progression node ${node.id} does not bind to its recipe: ${message}`, {
          cause: error,
        });
      }
      if (node.level === 1 && binding.plan.recipe.task.mode === 'action') {
        const repair = resolveSelections(binding, definition, [node.id], [node.id],
          node.gradingChecks.map(check => ({ ...check, nodeId: node.id })), 'upgrade');
        for (const reference of node.featureRefs) {
          const featureId = exactRef(reference).id;
          const fragments: Array<{
            kind: 'requirements' | 'contracts';
            ids: string[];
          }> = [
            { kind: 'requirements', ids: repair.agent.task.requirementIds },
            { kind: 'contracts', ids: repair.agent.task.contractIds },
          ];
          for (const { kind, ids } of fragments) {
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
