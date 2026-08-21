import { canonicalDefinitionJson } from './definition-plan.mjs';
import { sha256 } from '../evidence/provenance.mjs';

function unique(values, label) {
  const normalized = values.map(value => String(value).trim()).filter(Boolean);
  const seen = new Set();
  for (const value of normalized) {
    if (seen.has(value)) throw new Error(`${label} repeats ${value}`);
    seen.add(value);
  }
  return normalized;
}

function exactModuleRef(value, label) {
  const split = value.lastIndexOf('@');
  if (split < 1 || split === value.length - 1) {
    throw new Error(`${label} must use an exact id@version reference`);
  }
  return { id: value.slice(0, split), version: value.slice(split + 1), ref: value };
}

// Modular recipes keep product features and production specifications
// independently selectable. A specification has exactly one treatment:
// requested (prompt + score + repair), expected (score + repair, no initial
// disclosure), observed (separate first-build measurement), or excluded by
// omission. The closed treatment set prevents contradictory boolean policies.
export function resolveModularRecipeSelection(release, {
  featureIds = [], requestedSpecifications = [], expectedSpecifications = [],
  observedSpecifications = [], checkKeys = [],
} = {}) {
  if (!release?.contentSha256 || !Array.isArray(release.checkCatalog)
    || !Array.isArray(release.components?.packs)) {
    throw new Error('modular selection requires a compiled recipe release');
  }
  const modules = release.components.packs;
  if (!modules.length || modules.some(module => !['feature', 'specification'].includes(module.moduleType))) {
    throw new Error('modular selection requires every recipe module to declare feature or specification');
  }
  const features = new Map(modules.filter(module => module.moduleType === 'feature')
    .map(module => [module.id, module]));
  const specifications = new Map(modules.filter(module => module.moduleType === 'specification')
    .map(module => [`${module.id}@${module.version}`, module]));
  const requestedFeatures = unique(featureIds, 'features');
  const inputTreatments = {
    requested: unique(requestedSpecifications, 'requested specifications'),
    expected: unique(expectedSpecifications, 'expected specifications'),
    observed: unique(observedSpecifications, 'observed specifications'),
  };
  const selectedCheckKeys = unique(checkKeys, 'selected scored checks');
  const assigned = new Map();
  for (const [treatment, refs] of Object.entries(inputTreatments)) {
    for (const ref of refs) {
      if (assigned.has(ref)) {
        throw new Error(`specification ${ref} cannot be both ${assigned.get(ref)} and ${treatment}`);
      }
      assigned.set(ref, treatment);
    }
  }
  for (const id of requestedFeatures) {
    if (!features.has(id)) throw new Error(`recipe has no feature module ${id}`);
  }
  for (const [treatment, refs] of Object.entries(inputTreatments)) {
    for (const value of refs) {
      const parsed = exactModuleRef(value, `${treatment} specification`);
      if (!specifications.has(parsed.ref)) {
        throw new Error(`recipe has no ${treatment} specification ${parsed.ref}`);
      }
    }
  }

  const featureSet = new Set(requestedFeatures.length ? requestedFeatures : features.keys());
  const treatmentSets = Object.fromEntries(Object.entries(inputTreatments)
    .map(([treatment, refs]) => [treatment, new Set(refs)]));
  const moduleByRef = new Map(modules.map(module => [`${module.id}@${module.version}`, module]));
  const visit = (module, target, chain = []) => {
    const ref = `${module.id}@${module.version}`;
    if (chain.includes(ref)) throw new Error(`recipe module dependency cycle: ${[...chain, ref].join(' -> ')}`);
    for (const requiredRef of module.requiresPacks ?? []) {
      const required = moduleByRef.get(requiredRef);
      if (!required) throw new Error(`recipe module ${ref} requires missing ${requiredRef}`);
      if (required.moduleType === 'feature' && target === 'feature') featureSet.add(required.id);
      else if (required.moduleType === 'feature') {
        throw new Error(`specification module ${ref} cannot add feature ${requiredRef}; use check applicability`);
      } else if (target === 'feature') {
        throw new Error(`feature module ${ref} cannot depend on specification ${requiredRef}`);
      } else treatmentSets[target].add(requiredRef);
      visit(required, required.moduleType === 'feature' ? 'feature' : target, [...chain, ref]);
    }
  };
  for (const id of [...featureSet]) visit(features.get(id), 'feature');
  for (const [treatment, refs] of Object.entries(treatmentSets)) {
    for (const ref of [...refs]) visit(specifications.get(ref), treatment);
  }
  const resolvedAssignments = new Map();
  for (const [treatment, refs] of Object.entries(treatmentSets)) {
    for (const ref of refs) {
      if (resolvedAssignments.has(ref)) {
        throw new Error(`specification dependency ${ref} cannot be both ${resolvedAssignments.get(ref)} and ${treatment}`);
      }
      resolvedAssignments.set(ref, treatment);
    }
  }

  const idsFor = treatment => new Set([...treatmentSets[treatment]]
    .map(ref => exactModuleRef(ref, `${treatment} specification`).id));
  const requestedIds = idsFor('requested');
  const expectedIds = idsFor('expected');
  const observedIds = idsFor('observed');
  const promptPacks = new Set([...featureSet, ...requestedIds]);
  const applies = check => check.requiresFeatures === undefined
    || check.requiresFeatures.every(featureId => featureSet.has(featureId));
  const selectChecks = (ids, observation, treatment) => release.checkCatalog
    .filter(check => ids.has(check.packId) && applies(check)
      && (check.observations === undefined
        ? observation === 'requested' : check.observations.includes(observation)))
    .map(check => ({ ...check, treatment }));
  const pointBearing = checks => checks.filter(check => check.points > 0);
  const eligibleFeatureChecks = selectChecks(featureSet, 'requested', 'requested');
  const eligibleRequestedSpecChecks = selectChecks(requestedIds, 'requested', 'requested');
  const eligibleExpectedChecks = selectChecks(expectedIds, 'unmentioned', 'expected');
  const observedChecks = selectChecks(observedIds, 'unmentioned', 'observed');
  const featureChecks = pointBearing(eligibleFeatureChecks);
  const requestedSpecChecks = pointBearing(eligibleRequestedSpecChecks);
  const expectedChecks = pointBearing(eligibleExpectedChecks);
  const checksByTreatment = { requested: eligibleRequestedSpecChecks,
    expected: eligibleExpectedChecks,
    observed: observedChecks };
  for (const [treatment, refs] of Object.entries(treatmentSets)) {
    for (const ref of refs) {
      const id = exactModuleRef(ref, `${treatment} specification`).id;
      if (!checksByTreatment[treatment].some(check => check.packId === id)) {
        const observation = treatment === 'requested'
          ? 'prompted evaluation' : 'evaluation without prompting';
        throw new Error(`${treatment} specification ${ref} has no ${observation}`);
      }
    }
  }
  const allEligibleChecks = [...eligibleFeatureChecks, ...eligibleRequestedSpecChecks,
    ...eligibleExpectedChecks];
  const allScoredChecks = [...featureChecks, ...requestedSpecChecks, ...expectedChecks];
  const availableChecks = new Set(allEligibleChecks.map(check => check.stableKey));
  for (const key of selectedCheckKeys) {
    if (!availableChecks.has(key)) {
      throw new Error(`check ${key} is outside the requested/expected feature and specification scope`);
    }
  }
  const scoredChecks = selectedCheckKeys.length
    ? allEligibleChecks.filter(check => selectedCheckKeys.includes(check.stableKey)) : allScoredChecks;
  if (!scoredChecks.length) throw new Error('modular selection contains no scored checks');
  const observedKeys = new Set(observedChecks.map(check => check.stableKey));
  const checkOverlap = scoredChecks.filter(check => observedKeys.has(check.stableKey));
  if (checkOverlap.length) throw new Error('scored and observed checks must be disjoint');

  const identityDocument = {
    schemaVersion: 3,
    recipeContentSha256: release.contentSha256,
    features: [...featureSet].sort(),
    specifications: Object.fromEntries(Object.entries(treatmentSets)
      .map(([treatment, refs]) => [treatment, [...refs].sort()])),
    scoredChecks: scoredChecks.map(check => check.stableKey).sort(),
    observedChecks: observedChecks.map(check => check.stableKey).sort(),
  };
  const taskSelectionDocument = {
    schemaVersion: 3,
    recipeContentSha256: release.contentSha256,
    promptPacks: [...promptPacks].sort(),
  };
  const requested = {
    features: [...requestedFeatures].sort(),
    specifications: Object.fromEntries(Object.entries(inputTreatments)
      .map(([treatment, refs]) => [treatment, [...refs].sort()])),
    checks: [...selectedCheckKeys].sort(),
  };
  return {
    schemaVersion: 3,
    recipe: { id: release.id, version: release.version, contentSha256: release.contentSha256 },
    requested,
    features: [...featureSet].sort(),
    specifications: identityDocument.specifications,
    promptPacks: [...promptPacks].sort(),
    scoredChecks,
    checks: scoredChecks,
    observedChecks,
    scoredPoints: scoredChecks.reduce((total, check) => total + check.points, 0),
    taskSelectionSha256: sha256(canonicalDefinitionJson(taskSelectionDocument)),
    sha256: sha256(canonicalDefinitionJson(identityDocument)),
  };
}

// Resolve a caller's pack/check request once, then pass this exact result to
// every consumer. Packs define the requested task. Checks may narrow grading
// inside that task, but cannot silently add behavior the agent was not asked
// to build.
export function resolveRecipeSelection(release, { packIds = [], checkKeys = [] } = {}) {
  if (!release?.contentSha256 || !Array.isArray(release.checkCatalog)
    || !Array.isArray(release.components?.packs)) {
    throw new Error('recipe selection requires a compiled recipe release');
  }
  const requestedPacks = unique(packIds, '--pack');
  const requestedChecks = unique(checkKeys, '--check');
  const availablePacks = new Map(release.components.packs.map(pack => [pack.id, pack]));
  const availableChecks = new Set(release.checkCatalog.map(check => check.stableKey));
  for (const id of requestedPacks) if (!availablePacks.has(id)) throw new Error(`recipe has no pack ${id}`);
  for (const key of requestedChecks) if (!availableChecks.has(key)) throw new Error(`recipe has no check ${key}`);

  // No --pack means the recipe's complete requested task. A pack selection is
  // a smaller requested task, closed over declared dependencies. Checks never
  // add requirements: they may only narrow measurement inside that task.
  const taskPacks = new Set(requestedPacks.length ? requestedPacks : availablePacks.keys());
  const visit = (id, chain = []) => {
    if (chain.includes(id)) throw new Error(`recipe pack dependency cycle: ${[...chain, id].join(' -> ')}`);
    const pack = availablePacks.get(id);
    if (!pack) throw new Error(`recipe pack dependency is missing ${id}`);
    if (!Array.isArray(pack.requiresPacks)) {
      throw new Error(`recipe pack ${id} has no dependency metadata`);
    }
    for (const reference of pack.requiresPacks) {
      const split = String(reference).lastIndexOf('@');
      const requiredId = String(reference).slice(0, split);
      const version = String(reference).slice(split + 1);
      const required = availablePacks.get(requiredId);
      if (split < 1 || !required || required.version !== version) {
        throw new Error(`recipe pack ${id} requires missing ${reference}`);
      }
      taskPacks.add(requiredId);
      visit(requiredId, [...chain, id]);
    }
  };
  [...taskPacks].forEach(id => visit(id));

  for (const key of requestedChecks) {
    const check = release.checkCatalog.find(candidate => candidate.stableKey === key);
    if (!taskPacks.has(check.packId)) {
      throw new Error(`check ${key} belongs to unrequested pack ${check.packId}`);
    }
  }

  const checks = new Set(requestedChecks);
  const benchmarkChecks = release.checkCatalog.filter(check => check.points > 0);
  const selected = release.checkCatalog.filter(check => taskPacks.has(check.packId)
    && (checks.size ? checks.has(check.stableKey) : check.points > 0));
  if (!selected.length) throw new Error('pack/check request selects no checks');
  const selectedKeys = new Set(selected.map(check => check.stableKey));

  const identityDocument = {
    schemaVersion: 1,
    recipeContentSha256: release.contentSha256,
    taskPacks: [...taskPacks].sort(),
    checks: selected.map(check => check.stableKey).sort(),
  };
  return {
    schemaVersion: 1,
    recipe: { id: release.id, version: release.version, contentSha256: release.contentSha256 },
    requested: { packs: [...requestedPacks].sort(), checks: [...requestedChecks].sort() },
    taskPacks: [...taskPacks].sort(),
    sha256: sha256(canonicalDefinitionJson(identityDocument)),
    completeness: benchmarkChecks.every(check => selectedKeys.has(check.stableKey)) ? 'full' : 'subset',
    scoredPoints: selected.reduce((total, check) => total + check.points, 0),
    checks: selected.map(({ stableKey, executionId, packId, checkGroupId, source,
      featureId, criterionId, description, points }) => ({ stableKey, executionId, packId,
      checkGroupId, source, featureId, criterionId, description, points })),
  };
}

export function selectRecipeRelease(release, options = {}) {
  const selection = resolveRecipeSelection(release, options);
  const selectedKeys = new Set(selection.checks.map(check => check.stableKey));
  const selectedPacks = new Set(selection.taskPacks);
  return {
    ...release,
    selection,
    components: { ...release.components,
      packs: release.components.packs.filter(pack => selectedPacks.has(pack.id)) },
    checkCatalog: release.checkCatalog.filter(check => selectedKeys.has(check.stableKey)),
  };
}

export function composeSelectedRecipeTask(plan, selection) {
  const taskPacks = selection?.schemaVersion === 3 ? selection.promptPacks : selection?.taskPacks;
  if (!plan?.recipe?.task || !Array.isArray(taskPacks)
    || typeof selection.sha256 !== 'string') {
    throw new Error('selected task requires a compiled recipe plan and selection');
  }
  const owners = new Set(['recipe', ...taskPacks]);
  const features = new Set(selection.features ?? []);
  const select = fragments => fragments.filter(fragment =>
    fragment.owners.some(owner => owners.has(owner))
    && (fragment.requiresFeatures === undefined
      || fragment.requiresFeatures.every(featureId => features.has(featureId))));
  const requirements = select(plan.recipe.task.requirements);
  const contracts = select(plan.recipe.task.contracts);
  const compose = fragments => selection.schemaVersion === 3
    ? `${fragments.map(fragment => fragment.text.trimEnd()).join('\n\n')}\n`
    : fragments.map(fragment => fragment.text).join('');
  const requirementText = compose(requirements);
  const contractText = compose(contracts);
  const identity = {
    schemaVersion: 1,
    recipeContentSha256: selection.recipe.contentSha256,
    selectionSha256: selection.taskSelectionSha256 ?? selection.sha256,
    requirementIds: requirements.map(fragment => fragment.id),
    contractIds: contracts.map(fragment => fragment.id),
    requirementSha256: sha256(requirementText),
    contractSha256: sha256(contractText),
  };
  return { ...identity, sha256: sha256(canonicalDefinitionJson(identity)),
    requirementText, contractText };
}

function same(left, right) {
  return canonicalDefinitionJson(left) === canonicalDefinitionJson(right);
}

export function createRecipeTaskRequest(binding, options = {}) {
  if (!binding?.release || !binding?.plan) {
    throw new Error('recipe task request requires a resolved recipe binding');
  }
  const selection = resolveRecipeSelection(binding.release, options);
  const task = composeSelectedRecipeTask(binding.plan, selection);
  const request = {
    schemaVersion: 1,
    recipe: { id: binding.release.id, version: binding.release.version,
      contentSha256: binding.release.contentSha256 },
    selection: { sha256: selection.sha256, requested: selection.requested,
      taskPacks: selection.taskPacks },
    task: { sha256: task.sha256, requirementSha256: task.requirementSha256,
      contractSha256: task.contractSha256 },
  };
  return { request, selection, task };
}

export function createModularRecipeTaskRequest(binding, options = {}) {
  if (!binding?.release || !binding?.plan) {
    throw new Error('modular recipe task request requires a resolved recipe binding');
  }
  const selection = resolveModularRecipeSelection(binding.release, options);
  const task = composeSelectedRecipeTask(binding.plan, selection);
  const request = {
    schemaVersion: 3,
    recipe: { id: binding.release.id, version: binding.release.version,
      contentSha256: binding.release.contentSha256 },
    selection: {
      sha256: selection.sha256,
      requested: selection.requested,
      promptPacks: selection.promptPacks,
      resolved: { features: selection.features, specifications: selection.specifications,
        promptPacks: selection.promptPacks, taskSelectionSha256: selection.taskSelectionSha256 },
      scoredChecks: selection.scoredChecks.map(check => check.stableKey),
      observedChecks: selection.observedChecks.map(check => check.stableKey),
    },
    task: { sha256: task.sha256, requirementSha256: task.requirementSha256,
      contractSha256: task.contractSha256 },
  };
  return { request, selection, task };
}

export function resolveRecipeTaskRequest(binding, request) {
  if (!request || typeof request !== 'object' || Array.isArray(request)
    || request.schemaVersion !== 1 || !request.recipe || !request.selection || !request.task) {
    throw new Error('recipe task request is invalid');
  }
  const resolved = createRecipeTaskRequest(binding, {
    packIds: request.selection.requested?.packs,
    checkKeys: request.selection.requested?.checks,
  });
  if (!same(resolved.request, request)) {
    throw new Error('recipe task changed after request resolution');
  }
  return resolved;
}

export function resolveModularRecipeTaskRequest(binding, request) {
  if (!request || typeof request !== 'object' || Array.isArray(request)
    || request.schemaVersion !== 3 || !request.recipe || !request.selection || !request.task) {
    throw new Error('modular recipe task request is invalid');
  }
  const requested = request.selection.requested;
  const resolved = createModularRecipeTaskRequest(binding, {
    featureIds: requested?.features,
    requestedSpecifications: requested?.specifications?.requested,
    expectedSpecifications: requested?.specifications?.expected,
    observedSpecifications: requested?.specifications?.observed,
    checkKeys: requested?.checks,
  });
  if (!same(resolved.request, request)) {
    throw new Error('modular recipe task changed after request resolution');
  }
  return resolved;
}

export function isModularRecipeRelease(release) {
  const modules = release?.components?.packs;
  return Array.isArray(modules) && modules.length > 0
    && modules.every(module => ['feature', 'specification'].includes(module.moduleType));
}

export function createBoundRecipeTaskRequest(binding, options = {}) {
  if (!binding?.release) throw new Error('bound recipe task requires a recipe release');
  if (isModularRecipeRelease(binding.release)) {
    if ((options.packIds ?? []).length) throw new Error('modular recipes use feature modules, not packs');
    return createModularRecipeTaskRequest(binding, options);
  }
  if ((options.featureIds ?? []).length || (options.requestedSpecifications ?? []).length
    || (options.expectedSpecifications ?? []).length
    || (options.observedSpecifications ?? []).length) {
    throw new Error('legacy recipes do not support modular feature/specification selection');
  }
  return createRecipeTaskRequest(binding, options);
}

export function resolveBoundRecipeTaskRequest(binding, request) {
  if (!binding?.release) throw new Error('bound recipe task requires a recipe release');
  if (isModularRecipeRelease(binding.release)) {
    if (request?.schemaVersion !== 3) throw new Error('modular recipe requires a schema-3 task request');
    return resolveModularRecipeTaskRequest(binding, request);
  }
  if (request?.schemaVersion !== 1) throw new Error('legacy recipe requires a schema-1 task request');
  return resolveRecipeTaskRequest(binding, request);
}

// The controller owns undisclosed treatments. A coding process receives only
// requested specifications: expected and observed ids must not leak through
// argv, logs, process inspection, or adapter metadata. Removing them must not
// change the composed task because neither contributes prompt fragments.
export function createAgentVisibleTaskRequest(binding, selected) {
  const resolved = resolveBoundRecipeTaskRequest(binding, selected?.request ?? selected);
  if (resolved.request.schemaVersion === 1) return resolved.request;
  const requested = resolved.selection.requested;
  const visible = createBoundRecipeTaskRequest(binding, {
    featureIds: requested.features,
    requestedSpecifications: requested.specifications.requested,
    expectedSpecifications: [],
    observedSpecifications: [],
    // Check filters control grading, never prompt composition. Keeping them in
    // the coding-process request could disclose an expected check name after
    // its owning specification has been deliberately removed.
    checkKeys: [],
  });
  if (visible.task.sha256 !== resolved.task.sha256) {
    throw new Error('undisclosed treatment removal changed the agent task');
  }
  return visible.request;
}

// Filter a compiled scenario by stable recipe keys. This validates the entire
// mapping before a browser is launched, so a stale or cross-suite selection
// cannot turn into a plausible-looking partial score.
export function selectScenarioChecks(spec, recipeRelease, selectedStableKeys = []) {
  const requested = unique(selectedStableKeys, '--selected-check');
  if (!requested.length) return { features: spec.features, checks: recipeRelease?.checks ?? [] };
  if (!recipeRelease?.checks) throw new Error('selected checks require a recipe-bound scenario');
  const byKey = new Map(recipeRelease.checks.map(check => [check.stableKey, check]));
  const selected = requested.map(key => {
    const check = byKey.get(key);
    if (!check) throw new Error(`recipe execution has no selected check ${key}`);
    return check;
  });
  const selectedCriteria = new Map();
  for (const check of selected) {
    const key = String(check.featureId);
    const criteria = selectedCriteria.get(key) ?? new Set();
    criteria.add(String(check.criterionId));
    selectedCriteria.set(key, criteria);
  }
  const features = spec.features.flatMap(feature => {
    const criteria = selectedCriteria.get(String(feature.id));
    if (!criteria) return [];
    const filtered = feature.criteria.filter(criterion => criteria.has(String(criterion.id)));
    if (filtered.length !== criteria.size) {
      const found = new Set(filtered.map(criterion => String(criterion.id)));
      const missing = [...criteria].filter(id => !found.has(id));
      throw new Error(`scenario feature ${feature.id} has no selected criterion ${missing.join(', ')}`);
    }
    return [{ ...feature, criteria: filtered }];
  });
  const foundFeatures = new Set(features.map(feature => String(feature.id)));
  const missingFeatures = [...selectedCriteria.keys()].filter(id => !foundFeatures.has(id));
  if (missingFeatures.length) throw new Error(`scenario has no selected feature ${missingFeatures.join(', ')}`);
  return { features, checks: selected };
}
