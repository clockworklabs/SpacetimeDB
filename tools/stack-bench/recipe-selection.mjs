import { canonicalDefinitionJson } from './definition-plan.mjs';
import { sha256 } from './provenance.mjs';

function unique(values, label) {
  const normalized = values.map(value => String(value).trim()).filter(Boolean);
  const seen = new Set();
  for (const value of normalized) {
    if (seen.has(value)) throw new Error(`${label} repeats ${value}`);
    seen.add(value);
  }
  return normalized;
}

// Resolve a caller's pack/check request once, then pass this exact result to
// every consumer. Packs and individual checks are a union: asking for a pack
// plus one extra check means all checks in that pack plus the extra check.
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
  const selected = release.checkCatalog.filter(check => taskPacks.has(check.packId)
    && (!checks.size || checks.has(check.stableKey)));
  if (!selected.length) throw new Error('pack/check request selects no checks');

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
    completeness: selected.length === release.checkCatalog.length ? 'full' : 'subset',
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
  if (!plan?.recipe?.task || !Array.isArray(selection?.taskPacks)
    || typeof selection.sha256 !== 'string') {
    throw new Error('selected task requires a compiled recipe plan and selection');
  }
  const owners = new Set(['recipe', ...selection.taskPacks]);
  const select = fragments => fragments.filter(fragment =>
    fragment.owners.some(owner => owners.has(owner)));
  const requirements = select(plan.recipe.task.requirements);
  const contracts = select(plan.recipe.task.contracts);
  const requirementText = requirements.map(fragment => fragment.text).join('');
  const contractText = contracts.map(fragment => fragment.text).join('');
  const identity = {
    schemaVersion: 1,
    recipeContentSha256: selection.recipe.contentSha256,
    selectionSha256: selection.sha256,
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
