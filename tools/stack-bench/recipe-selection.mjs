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
  const packs = new Set(requestedPacks);
  const checks = new Set(requestedChecks);
  const availablePacks = new Set(release.components.packs.map(pack => pack.id));
  const availableChecks = new Set(release.checkCatalog.map(check => check.stableKey));
  for (const id of packs) if (!availablePacks.has(id)) throw new Error(`recipe has no pack ${id}`);
  for (const key of checks) if (!availableChecks.has(key)) throw new Error(`recipe has no check ${key}`);

  const selectAll = packs.size === 0 && checks.size === 0;
  const selected = release.checkCatalog.filter(check => selectAll
    || packs.has(check.packId) || checks.has(check.stableKey));
  if (!selected.length) throw new Error('pack/check request selects no checks');

  const identityDocument = {
    schemaVersion: 1,
    recipeContentSha256: release.contentSha256,
    checks: selected.map(check => check.stableKey).sort(),
  };
  return {
    schemaVersion: 1,
    recipe: { id: release.id, version: release.version, contentSha256: release.contentSha256 },
    requested: { packs: [...requestedPacks].sort(), checks: [...requestedChecks].sort() },
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
  const selectedPacks = new Set(selection.checks.map(check => check.packId));
  return {
    ...release,
    selection,
    components: { ...release.components,
      packs: release.components.packs.filter(pack => selectedPacks.has(pack.id)) },
    checkCatalog: release.checkCatalog.filter(check => selectedKeys.has(check.stableKey)),
  };
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
