import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';

import { compilePromotionFile, compileRecipeFile } from './composition-compiler.mjs';
import { compileScenarioDefinition, compileTrackManifest } from './definition-compiler.mjs';
import { canonicalDefinitionJson, canonicalizeDefinition } from './definition-plan.mjs';
import { sha256 } from './provenance.mjs';
import { suitesFor } from './tracks.mjs';

export const RECIPE_RELEASE_SCHEMA_VERSION = 1;

const SHA256 = /^[a-f0-9]{64}$/;
const EXECUTION_ONLY_STEP_FIELDS = new Set([
  'actor', 'actors', 'testid', 'in', 'within', 'settleMs', 'ms', 'intervalMs',
  'delayMs', 'readyTestid', 'samples', 'secondsAhead',
]);
const ASSERTION_CONTAINS_ACTIONS = new Set([
  'expect', 'expectActorsWith', 'expectElementCount', 'expectNotReceived', 'expectReceived',
]);

function readJson(path, label) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    throw new Error(`cannot read ${label} ${path}: ${error.message}`, { cause: error });
  }
}

function trackRelative(trackRoot, path) {
  return relative(resolve(trackRoot), realpathSync(path)).replaceAll('\\', '/');
}

function semanticStep(step) {
  if (step.do === 'race') {
    return {
      do: step.do,
      branches: step.branches.map(branch => branch.map(semanticStep)),
    };
  }
  return Object.fromEntries(Object.entries(step)
    .filter(([key]) => !EXECUTION_ONLY_STEP_FIELDS.has(key)
      && (key !== 'contains' || ASSERTION_CONTAINS_ACTIONS.has(step.do)))
    .map(([key, value]) => [key, canonicalizeDefinition(value)]));
}

function taskDocuments(plan, trackRoot) {
  const compact = fragment => ({
    id: fragment.id,
    path: fragment.path,
    order: fragment.order,
    from: fragment.from,
    until: fragment.until,
    modes: fragment.modes,
    owners: fragment.owners,
    ...(fragment.requiresFeatures === undefined
      ? {} : { requiresFeatures: fragment.requiresFeatures }),
    sha256: sha256(fragment.text),
    text: fragment.text,
  });
  return {
    requirements: plan.recipe.task.requirements.map(compact),
    contracts: plan.recipe.task.contracts.map(compact),
    requirementText: plan.recipe.task.requirementText,
    contractText: plan.recipe.task.contractText,
  };
}

function checkDetails(plan) {
  const details = [];
  for (const execution of plan.execution) {
    for (const group of execution.checkGroups) {
      for (const criterion of group.feature.criteria) {
        const stableKey = `${group.packId}.${group.checkGroupId}.${criterion.id}`;
        const compiled = plan.checks.find(check => check.stableKey === stableKey);
        if (!compiled) throw new Error(`compiled recipe lost check ${stableKey}`);
        details.push({
          stableKey,
          executionId: execution.id,
          packId: group.packId,
          packVersion: group.packVersion,
          checkGroupId: group.checkGroupId,
          role: group.role,
          ...(group.observations === undefined ? {} : { observations: group.observations }),
          ...(group.requiresFeatures === undefined ? {} : { requiresFeatures: group.requiresFeatures }),
          source: group.source,
          featureId: group.feature.id,
          featureName: group.feature.name,
          criterionId: criterion.id,
          description: criterion.desc,
          note: criterion.note ?? null,
          statedBy: criterion.statedBy ?? null,
          provenBy: criterion.provenBy ?? null,
          withheld: criterion.withheld ?? null,
          points: compiled.points,
          sourcePoints: compiled.sourcePoints,
          semantics: criterion.steps.map(semanticStep),
        });
      }
    }
  }
  return details;
}

function sourceEntry(trackRoot, path, kind) {
  const absolute = realpathSync(path);
  return { path: trackRelative(trackRoot, absolute), sha256: sha256(readFileSync(absolute)), kinds: [kind] };
}

function mergeSources(entries) {
  const merged = new Map();
  for (const entry of entries) {
    const current = merged.get(entry.path);
    if (current && current.sha256 !== entry.sha256) {
      throw new Error(`source ${entry.path} produced conflicting digests`);
    }
    if (current) current.kinds.push(...entry.kinds);
    else merged.set(entry.path, structuredClone(entry));
  }
  return [...merged.values()].map(entry => ({ ...entry, kinds: [...new Set(entry.kinds)].sort() }))
    .sort((a, b) => a.path.localeCompare(b.path));
}

function releaseIdentity(release) {
  return {
    recipeReleaseSchemaVersion: release.recipeReleaseSchemaVersion,
    id: release.id,
    version: release.version,
    state: release.state,
    track: release.track,
    meaningSha256: release.meaningSha256,
    executionSha256: release.executionSha256,
    contentSha256: release.contentSha256,
    sourceManifestSha256: release.sourceManifestSha256,
  };
}

export function recipeReleaseIdentity(release) {
  return canonicalizeDefinition(releaseIdentity(release));
}

export function buildRecipeRelease(recipePath, { trackRoot } = {}) {
  const absoluteRecipe = realpathSync(resolve(recipePath));
  const root = realpathSync(resolve(trackRoot ?? dirname(dirname(dirname(absoluteRecipe)))));
  const plan = compileRecipeFile(absoluteRecipe, { trackRoot: root });
  const rawRecipe = readJson(absoluteRecipe, 'recipe');
  const trackManifestPath = join(root, 'track.json');
  const trackManifest = compileTrackManifest(readJson(trackManifestPath, 'track manifest'), {
    source: trackManifestPath,
  });
  const documents = taskDocuments(plan, root);
  const details = checkDetails(plan);

  let baseRelease = null;
  if (plan.recipe.task.baseRecipe) {
    baseRelease = buildRecipeRelease(join(root, 'composition', plan.recipe.task.baseRecipe.path), {
      trackRoot: root,
    });
  }

  const meaningDocument = canonicalizeDefinition({
    schemaVersion: RECIPE_RELEASE_SCHEMA_VERSION,
    track: plan.recipe.track,
    task: {
      mode: plan.recipe.task.mode,
      baseMeaningSha256: baseRelease?.meaningSha256 ?? null,
      requirements: documents.requirements.map(({ id, owners, requiresFeatures, text }) => ({
        id, owners, ...(requiresFeatures === undefined ? {} : { requiresFeatures }), text,
      })),
      contracts: documents.contracts.map(({ id, owners, requiresFeatures, text }) => ({
        id, owners, ...(requiresFeatures === undefined ? {} : { requiresFeatures }), text,
      })),
    },
    checks: details.map(detail => ({
      stableKey: detail.stableKey,
      packId: detail.packId,
      checkGroupId: detail.checkGroupId,
      role: detail.role,
      ...(detail.observations === undefined ? {} : { observations: detail.observations }),
      ...(detail.requiresFeatures === undefined ? {} : { requiresFeatures: detail.requiresFeatures }),
      source: detail.source,
      featureId: detail.featureId,
      featureName: detail.featureName,
      criterionId: detail.criterionId,
      description: detail.description,
      note: detail.note,
      statedBy: detail.statedBy,
      provenBy: detail.provenBy,
      withheld: detail.withheld,
      points: detail.points,
      semantics: detail.semantics,
    })),
  });

  const scenarioDefinitions = Object.fromEntries(plan.execution.map(execution => {
    const path = join(root, execution.source);
    const scenario = compileScenarioDefinition(readJson(path, 'scenario'), { source: path });
    return [execution.source, {
      level: scenario.level,
      writeUrlPattern: scenario.writeUrlPattern ?? null,
    }];
  }));
  const executionDocument = canonicalizeDefinition({
    schemaVersion: RECIPE_RELEASE_SCHEMA_VERSION,
    track: plan.recipe.track,
    task: {
      mode: plan.recipe.task.mode,
      baseExecutionSha256: baseRelease?.executionSha256 ?? null,
    },
    fixture: {
      warehouses: plan.fixture.warehouses,
      items: plan.fixture.items,
      accounts: plan.fixture.accounts,
      empty: plan.fixture.empty,
    },
    packs: plan.packs.map(pack => ({
      id: pack.id,
      ...(pack.moduleType === undefined ? {} : { moduleType: pack.moduleType }),
      includeRoles: [...pack.includeRoles].sort(),
      capabilities: [...pack.capabilities].sort(),
      evidence: [...pack.evidence].sort(),
      budget: pack.budget,
      actions: [...pack.actions].sort(),
    })).sort((a, b) => a.id.localeCompare(b.id)),
    capabilities: plan.capabilities,
    runtime: {
      portOffset: trackManifest.portOffset ?? 0,
      restartProbe: trackManifest.restartProbe ?? '/',
      reseedOnReset: trackManifest.reseedOnReset ?? false,
      actions: [...(trackManifest.actions ?? [])].sort((a, b) => a.id.localeCompare(b.id)),
    },
    execution: plan.execution.map(execution => ({
      id: execution.id,
      source: execution.source,
      scenario: scenarioDefinitions[execution.source],
      checkGroups: execution.checkGroups.map(group => ({
        packId: group.packId,
        checkGroupId: group.checkGroupId,
        role: group.role,
        ...(group.observations === undefined ? {} : { observations: group.observations }),
        ...(group.requiresFeatures === undefined ? {} : { requiresFeatures: group.requiresFeatures }),
        source: group.source,
        feature: {
          id: group.feature.id,
          actors: group.feature.actors,
          setup: group.feature.setup,
          criteria: group.feature.criteria.map(criterion => ({
            id: criterion.id,
            steps: criterion.steps,
          })),
        },
      })),
    })),
  });

  const meaningSha256 = sha256(canonicalDefinitionJson(meaningDocument));
  const executionSha256 = sha256(canonicalDefinitionJson(executionDocument));
  const contentSha256 = sha256(canonicalDefinitionJson({
    schemaVersion: RECIPE_RELEASE_SCHEMA_VERSION,
    meaningSha256,
    executionSha256,
  }));

  const fixturePath = realpathSync(resolve(dirname(absoluteRecipe), rawRecipe.fixture.path));
  const ownSources = [
    sourceEntry(root, trackManifestPath, 'track-manifest'),
    sourceEntry(root, absoluteRecipe, 'recipe'),
    sourceEntry(root, fixturePath, 'fixture'),
    ...plan.packs.map(pack => sourceEntry(root, join(root, 'composition', pack.path), 'pack')),
    ...plan.execution.map(execution => sourceEntry(root, join(root, execution.source), 'scenario')),
    ...documents.requirements.map(fragment => sourceEntry(root, join(root, fragment.path), 'requirement-source')),
    ...documents.contracts.map(fragment => sourceEntry(root, join(root, fragment.path), 'contract-source')),
  ];
  const sourceManifest = mergeSources([
    ...ownSources,
    ...(baseRelease?.sourceManifest ?? []),
  ]);
  const packSource = pack => {
    const path = trackRelative(root, join(root, 'composition', pack.path));
    const source = sourceManifest.find(entry => entry.path === path && entry.kinds.includes('pack'));
    if (!source) throw new Error(`recipe release lost pack source ${path}`);
    return { path, sha256: source.sha256 };
  };

  return canonicalizeDefinition({
    recipeReleaseSchemaVersion: RECIPE_RELEASE_SCHEMA_VERSION,
    id: plan.recipe.id,
    version: plan.recipe.version,
    state: plan.recipe.state,
    title: plan.recipe.title,
    track: plan.recipe.track,
    compatibility: plan.recipe.compatibility,
    task: {
      mode: plan.recipe.task.mode,
      baseRecipe: baseRelease ? recipeReleaseIdentity(baseRelease) : null,
      requirements: documents.requirements.map(({ text: _text, ...fragment }) => fragment),
      contracts: documents.contracts.map(({ text: _text, ...fragment }) => fragment),
      requirementSha256: sha256(documents.requirementText),
      contractSha256: sha256(documents.contractText),
      composedSha256: sha256(`${documents.requirementText}\n${documents.contractText}`),
    },
    components: {
      fixture: { id: plan.fixture.id, version: plan.fixture.version, state: plan.fixture.state,
        path: trackRelative(root, fixturePath), sha256: sha256(readFileSync(fixturePath)) },
      packs: plan.packs.map(pack => ({ id: pack.id, version: pack.version, state: pack.state,
        ...packSource(pack), includeRoles: [...pack.includeRoles].sort(),
        ...(pack.moduleType === undefined ? {} : { moduleType: pack.moduleType }),
        requiresPacks: [...pack.requiresPacks].sort() }))
        .sort((a, b) => a.id.localeCompare(b.id)),
    },
    capabilities: plan.capabilities,
    scoring: plan.scoring,
    meaningSha256,
    executionSha256,
    contentSha256,
    sourceManifestSha256: sha256(canonicalDefinitionJson(sourceManifest)),
    sourceManifest,
    checkCatalog: details.map(detail => ({
      stableKey: detail.stableKey,
      executionId: detail.executionId,
      packId: detail.packId,
      packVersion: detail.packVersion,
      checkGroupId: detail.checkGroupId,
      role: detail.role,
      ...(detail.observations === undefined ? {} : { observations: detail.observations }),
      ...(detail.requiresFeatures === undefined ? {} : { requiresFeatures: detail.requiresFeatures }),
      source: detail.source,
      featureId: detail.featureId,
      criterionId: detail.criterionId,
      description: detail.description,
      points: detail.points,
    })),
  });
}

function legacyProjection(track, level) {
  return suitesFor(track, level).map(suite => {
    const source = trackRelative(track.dir, suite.spec);
    const spec = compileScenarioDefinition(readJson(suite.spec, 'scenario'), { source });
    return {
      id: suite.id,
      source,
      features: spec.features.map(feature => ({ id: feature.id,
        criteria: feature.criteria.map(criterion => ({ id: criterion.id, points: criterion.points ?? 1 })) })),
    };
  });
}

function recipeProjection(plan) {
  return plan.execution.map(execution => ({
    id: execution.id,
    source: execution.source,
    features: execution.checkGroups.reduce((features, group) => {
      let feature = features.at(-1);
      if (!feature || feature.id !== group.feature.id) {
        feature = { id: group.feature.id, criteria: [] };
        features.push(feature);
      }
      feature.criteria.push(...group.feature.criteria.map(criterion => ({
        id: criterion.id, points: criterion.points ?? 1,
      })));
      return features;
    }, []),
  }));
}

export function assertLegacyRecipeParity(plan, track, level) {
  if (plan.recipe.compatibility?.legacyLevel !== Number(level)) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} does not declare L${level} compatibility`);
  }
  if (plan.scoring.mode !== 'legacy-source-points') {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} cannot use the legacy runner with ${plan.scoring.mode} scoring`);
  }
  const expected = canonicalDefinitionJson(legacyProjection(track, level));
  const actual = canonicalDefinitionJson(recipeProjection(plan));
  if (actual !== expected) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} does not exactly match the L${level} legacy execution plan`);
  }
}

function assertCompatibilityCandidateContinuity(plan, promotedPlan, track, level) {
  assertLegacyRecipeParity(promotedPlan, track, level);
  if (plan.recipe.compatibility?.legacyLevel !== Number(level)) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} does not declare L${level} compatibility`);
  }
  if (plan.scoring.mode !== 'legacy-source-points') {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} cannot use the legacy runner with ${plan.scoring.mode} scoring`);
  }
  const baseline = new Map(promotedPlan.checks.map(check => [check.stableKey, check]));
  const candidate = new Map(plan.checks.map(check => [check.stableKey, check]));
  const missing = [...baseline.keys()].filter(stableKey => !candidate.has(stableKey));
  const added = [...candidate.keys()].filter(stableKey => !baseline.has(stableKey));
  if (missing.length || added.length) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} changes the L${level} compatibility check set`);
  }
  for (const [stableKey, previous] of baseline) {
    const next = candidate.get(stableKey);
    if (next.points < previous.points || (previous.points > 0 && next.points !== previous.points)) {
      throw new Error(`${plan.recipe.id}@${plan.recipe.version} changes the established score for ${stableKey}`);
    }
  }
}

function cumulativeBasePlan(plan, track, level) {
  if (plan.recipe.compatibility?.legacyLevel !== Number(level)
      || plan.recipe.compatibility?.mode !== 'cumulative') {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} does not declare cumulative L${level} compatibility`);
  }
  const base = plan.recipe.task.baseRecipe;
  if (!base) throw new Error(`${plan.recipe.id}@${plan.recipe.version} has no cumulative base recipe`);
  const basePlan = compileRecipeFile(join(track.dir, 'composition', base.path), {
    trackRoot: track.dir,
  });
  const candidateByKey = new Map(plan.checks.map(check => [check.stableKey, check]));
  for (const check of basePlan.checks) {
    const carried = candidateByKey.get(check.stableKey);
    if (!carried || canonicalDefinitionJson(carried) !== canonicalDefinitionJson(check)) {
      throw new Error(`${plan.recipe.id}@${plan.recipe.version} does not carry base check ${check.stableKey} exactly`);
    }
  }
  return basePlan;
}

function executionStableKeys(execution) {
  return execution.checkGroups.flatMap(group => group.feature.criteria.map(criterion =>
    `${group.packId}.${group.checkGroupId}.${criterion.id}`));
}

function legacyExecutionPlan(plan, track, level) {
  const declared = new Map(suitesFor(track, level).map(suite => [suite.id, suite]));
  return plan.execution.map(execution => {
    const suite = declared.get(execution.id);
    if (!suite) {
      throw new Error(`${plan.recipe.id}@${plan.recipe.version} has no declared L${level} execution ${execution.id}`);
    }
    return {
      id: execution.id,
      source: execution.source,
      ownership: suite.inherited
        ? { kind: 'inherited', fromLevel: suite.fromLevel }
        : { kind: 'current', level: Number(level) },
    };
  });
}

// Ownership is release structure, not an execution-id naming convention. A
// cumulative recipe inherits every check selected by its exact base recipe;
// the remaining checks belong to the level being introduced. Recursing through
// bases preserves the original owner when L3 carries both L1 and L2 checks.
function cumulativeExecutionPlan(plan, track, level) {
  const base = plan.recipe.task.baseRecipe;
  if (!base) throw new Error(`${plan.recipe.id}@${plan.recipe.version} has no cumulative base recipe`);
  const basePlan = compileRecipeFile(join(track.dir, 'composition', base.path), {
    trackRoot: track.dir,
  });
  const declaredBaseLevel = basePlan.recipe.compatibility?.legacyLevel;
  const baseLevel = declaredBaseLevel ?? Number(level) - 1;
  if (!Number.isInteger(baseLevel) || baseLevel < 1 || baseLevel !== Number(level) - 1) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} must inherit exact L${Number(level) - 1}, `
      + `not L${baseLevel}`);
  }
  const baseExecution = executionPlanForRecipe(basePlan, track, baseLevel);
  const baseById = new Map(basePlan.execution.map(execution => [execution.id, execution]));
  const baseOrigins = new Map();
  for (const owned of baseExecution) {
    const execution = baseById.get(owned.id);
    if (!execution) throw new Error(`base recipe lost execution ${owned.id}`);
    const origin = owned.ownership.kind === 'inherited'
      ? owned.ownership.fromLevel : owned.ownership.level;
    for (const stableKey of executionStableKeys(execution)) baseOrigins.set(stableKey, origin);
  }

  return plan.execution.map(execution => {
    const stableKeys = executionStableKeys(execution);
    const inherited = stableKeys.filter(stableKey => baseOrigins.has(stableKey));
    if (inherited.length === 0) {
      return { id: execution.id, source: execution.source,
        ownership: { kind: 'current', level: Number(level) } };
    }
    if (inherited.length !== stableKeys.length) {
      throw new Error(`${plan.recipe.id}@${plan.recipe.version} execution ${execution.id} `
        + 'mixes inherited and current-level checks');
    }
    const origins = new Set(inherited.map(stableKey => baseOrigins.get(stableKey)));
    if (origins.size !== 1) {
      throw new Error(`${plan.recipe.id}@${plan.recipe.version} execution ${execution.id} `
        + `mixes checks owned by levels ${[...origins].sort((a, b) => a - b).join(', ')}`);
    }
    return { id: execution.id, source: execution.source,
      ownership: { kind: 'inherited', fromLevel: [...origins][0] } };
  });
}

export function executionPlanForRecipe(plan, track, level) {
  if (plan.recipe.compatibility?.mode === 'cumulative') {
    return cumulativeExecutionPlan(plan, track, level);
  }
  if (plan.recipe.compatibility !== null) return legacyExecutionPlan(plan, track, level);
  return plan.execution.map(execution => ({
    id: execution.id,
    source: execution.source,
    ownership: { kind: 'current', level: Number(level) },
  }));
}

export function executionPlanForRelease(recipePath, { trackRoot, level } = {}) {
  if (!Number.isInteger(Number(level)) || Number(level) < 1) {
    throw new Error('typed execution ownership requires a positive level');
  }
  const root = realpathSync(resolve(trackRoot));
  const plan = compileRecipeFile(recipePath, { trackRoot: root });
  const manifestPath = join(root, 'track.json');
  const manifest = compileTrackManifest(readJson(manifestPath, 'track manifest'), {
    source: manifestPath,
  });
  return executionPlanForRecipe(plan, {
    ...manifest,
    name: plan.recipe.track,
    dir: root,
  }, Number(level));
}

function assertInitialCumulativeBase(plan, promotionCatalog, track, level) {
  const numericLevel = Number(level);
  if (numericLevel <= 1) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} cannot bootstrap cumulative L${numericLevel}`);
  }
  const lowerAlias = `L${numericLevel - 1}`;
  const promotedBase = promotionCatalog.entries.filter(entry =>
    entry.alias === lowerAlias && entry.status === 'promoted');
  if (promotedBase.length !== 1) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} initial cumulative L${numericLevel} `
      + `requires exactly one promoted ${lowerAlias} base; found ${promotedBase.length}`);
  }
  const embedded = cumulativeBasePlan(plan, track, level);
  const selected = promotedBase[0];
  if (embedded.recipe.id !== selected.recipe.id || embedded.recipe.version !== selected.recipe.version) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} initial cumulative L${numericLevel} base `
      + `${embedded.recipe.id}@${embedded.recipe.version} is not promoted ${lowerAlias} `
      + `${selected.recipe.id}@${selected.recipe.version}`);
  }
  const embeddedRelease = buildRecipeRelease(join(track.dir, 'composition', plan.recipe.task.baseRecipe.path), {
    trackRoot: track.dir,
  });
  const promotedRelease = buildRecipeRelease(join(track.dir, 'composition', selected.recipe.path), {
    trackRoot: track.dir,
  });
  if (embeddedRelease.contentSha256 !== promotedRelease.contentSha256) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} initial cumulative L${numericLevel} `
      + `does not bind the exact promoted ${lowerAlias} content`);
  }
}

function assertCumulativeContinuity(plan, previousPlans, track, level) {
  if (previousPlans.length === 0) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} has no cumulative L${level} baseline`);
  }
  for (const previousPlan of previousPlans) {
    if (previousPlan.recipe.compatibility?.mode === 'cumulative') {
      cumulativeBasePlan(previousPlan, track, level);
    } else {
      assertLegacyRecipeParity(previousPlan, track, level);
    }
  }
  const basePlan = cumulativeBasePlan(plan, track, level);
  const retainedLevelChecks = previousPlans.flatMap(previousPlan => {
    const previousBase = previousPlan.recipe.task.baseRecipe;
    if (!previousBase) {
      throw new Error(`${previousPlan.recipe.id}@${previousPlan.recipe.version} has no L${level} base recipe`);
    }
    const previousBasePlan = compileRecipeFile(join(track.dir, 'composition', previousBase.path), {
      trackRoot: track.dir,
    });
    const previousBaseKeys = new Set(previousBasePlan.checks.map(check => check.stableKey));
    return previousPlan.checks.filter(check => !previousBaseKeys.has(check.stableKey));
  });
  const required = new Set([
    ...basePlan.checks.map(check => check.stableKey),
    ...retainedLevelChecks.map(check => check.stableKey),
  ]);
  const actual = new Set(plan.checks.map(check => check.stableKey));
  const missing = [...required].filter(stableKey => !actual.has(stableKey));
  const added = [...actual].filter(stableKey => !required.has(stableKey));
  if (missing.length || added.length) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} changes the cumulative L${level} check set`);
  }
  const candidate = new Map(plan.checks.map(check => [check.stableKey, check]));
  for (const previous of retainedLevelChecks) {
    const next = candidate.get(previous.stableKey);
    if (next.points < previous.points || (previous.points > 0 && next.points !== previous.points)) {
      throw new Error(`${plan.recipe.id}@${plan.recipe.version} changes the established score for ${previous.stableKey}`);
    }
  }
}

function exactRecipeRequest(requested) {
  if (requested === null || requested === undefined) return null;
  if (typeof requested === 'string') {
    const separator = requested.lastIndexOf('@');
    if (separator < 1 || separator === requested.length - 1) {
      throw new Error('--recipe must be an exact <id>@<version> reference');
    }
    return { id: requested.slice(0, separator), version: requested.slice(separator + 1) };
  }
  if (typeof requested !== 'object' || Array.isArray(requested)
    || typeof requested.id !== 'string' || typeof requested.version !== 'string') {
    throw new Error('exact recipe selection requires an id and version');
  }
  const fields = new Set(['id', 'version', 'contentSha256']);
  if (Object.keys(requested).some(field => !fields.has(field))) {
    throw new Error('exact recipe selection contains an unknown field');
  }
  if (Object.hasOwn(requested, 'contentSha256') && !SHA256.test(requested.contentSha256)) {
    throw new Error('exact recipe selection contentSha256 must be a SHA-256 digest');
  }
  return { id: requested.id, version: requested.version,
    ...(requested.contentSha256 !== undefined ? { contentSha256: requested.contentSha256 } : {}) };
}

// Normal runs resolve the promoted L<n> alias. Qualification may name one
// catalogued candidate exactly, so it can be tested before that alias moves.
// Both choices return the same binding and use the same runner path.
export function resolveRecipeRelease(track, level, requested = null) {
  const catalogPath = join(track.dir, 'composition', 'promotions.json');
  if (!existsSync(catalogPath)) return null;
  let selectedCatalogPath = catalogPath;
  const promotionCatalog = compilePromotionFile(catalogPath, { trackRoot: track.dir });
  let catalog = promotionCatalog;
  const alias = `L${Number(level)}`;
  const exact = exactRecipeRequest(requested);
  if (!exact && !catalog.entries.some(entry => entry.alias === alias)) return null;
  const promoted = catalog.entries.filter(entry => entry.alias === alias && entry.status === 'promoted');
  const candidates = catalog.entries.filter(entry => entry.alias === alias && entry.status === 'candidate');
  let choices = exact
    ? catalog.entries.filter(entry => entry.alias === alias && entry.recipe.id === exact.id
      && entry.recipe.version === exact.version && entry.status !== 'retired')
    : (promoted.length ? promoted : candidates);
  const candidateCatalogPath = join(track.dir, 'composition', 'candidates.json');
  if (exact && choices.length === 0 && existsSync(candidateCatalogPath)) {
    const candidateCatalog = compilePromotionFile(candidateCatalogPath, { trackRoot: track.dir });
    choices = candidateCatalog.entries.filter(entry => entry.alias === alias
      && entry.recipe.id === exact.id && entry.recipe.version === exact.version
      && entry.status === 'candidate');
    if (choices.length) {
      catalog = candidateCatalog;
      selectedCatalogPath = candidateCatalogPath;
    }
  }
  if (choices.length !== 1) {
    const kind = exact ? `catalogued ${exact.id}@${exact.version}`
      : `${promoted.length ? 'promoted' : 'candidate'} recipe`;
    throw new Error(`${alias} requires exactly one ${kind}; found ${choices.length}`);
  }
  const selection = choices[0];
  const recipePath = join(track.dir, 'composition', selection.recipe.path);
  const plan = compileRecipeFile(recipePath, { trackRoot: track.dir });
  const compatibilityMode = plan.recipe.compatibility?.mode ?? 'legacy-parity';
  if (plan.recipe.compatibility !== null && selection.status === 'candidate') {
    if (compatibilityMode === 'cumulative') {
      if (promoted.length === 0) assertInitialCumulativeBase(plan, promotionCatalog, track, level);
      else {
        const promotedPlan = compileRecipeFile(join(track.dir, 'composition', promoted[0].recipe.path),
          { trackRoot: track.dir });
        assertCumulativeContinuity(plan, [promotedPlan], track, level);
      }
    } else {
      if (promoted.length !== 1) {
        throw new Error(`${alias} compatibility candidate requires exactly one promoted baseline; found ${promoted.length}`);
      }
      const promotedPlan = compileRecipeFile(join(track.dir, 'composition', promoted[0].recipe.path),
        { trackRoot: track.dir });
      assertCompatibilityCandidateContinuity(plan, promotedPlan, track, level);
    }
  } else if (plan.recipe.compatibility !== null) {
    if (compatibilityMode === 'cumulative') {
      const previousPlans = promotionCatalog.entries
        .filter(entry => entry.alias === alias && entry.status === 'retired')
        .map(entry => compileRecipeFile(join(track.dir, 'composition', entry.recipe.path),
          { trackRoot: track.dir }));
      if (previousPlans.length === 0) assertInitialCumulativeBase(plan, promotionCatalog, track, level);
      else assertCumulativeContinuity(plan, previousPlans, track, level);
    }
    else assertLegacyRecipeParity(plan, track, level);
  }
  else if (!plan.packs.length
    || plan.packs.some(pack => !['feature', 'specification'].includes(pack.moduleType))) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} is neither a compatibility recipe nor modular`);
  }
  const release = buildRecipeRelease(recipePath, { trackRoot: track.dir });
  if (exact?.contentSha256 && release.contentSha256 !== exact.contentSha256) {
    throw new Error(`${exact.id}@${exact.version} content changed: expected ${exact.contentSha256}, `
      + `resolved ${release.contentSha256}`);
  }
  return {
    alias,
    status: selection.status,
    catalog: { ...catalog.catalog, path: trackRelative(track.dir, selectedCatalogPath),
      sha256: sha256(readFileSync(selectedCatalogPath)) },
    recipePath,
    plan,
    execution: executionPlanForRecipe(plan, track, level),
    release,
  };
}

export function gradeRecipeRelease(binding, executionId, featureId = null) {
  if (!binding) return null;
  const checks = binding.release.checkCatalog.filter(check => check.executionId === executionId
    && (featureId === null || check.featureId === featureId));
  if (!checks.length) throw new Error(`recipe ${binding.release.id} has no execution ${executionId}`);
  return canonicalizeDefinition({
    ...recipeReleaseIdentity(binding.release),
    selection: { alias: binding.alias, status: binding.status, catalog: binding.catalog },
    executionId,
    checks,
  });
}

export function resolveGradeRecipeArtifactBinding(track, level, specPath, featureId = null,
  requested = null) {
  const binding = resolveRecipeRelease(track, level, requested);
  if (!binding) return null;
  const absoluteSpec = realpathSync(specPath);
  const execution = binding.plan.execution.find(candidate =>
    realpathSync(join(track.dir, candidate.source)) === absoluteSpec);
  if (!execution) {
    throw new Error(`recipe ${binding.release.id}@${binding.release.version} does not select scenario ${specPath}`);
  }
  return {
    release: gradeRecipeRelease(binding, execution.id, featureId),
    sourceRelease: binding.release,
  };
}

export function resolveGradeRecipeRelease(track, level, specPath, featureId = null, requested = null) {
  return resolveGradeRecipeArtifactBinding(track, level, specPath, featureId, requested)?.release ?? null;
}

export function bundleRecipeRelease(binding) {
  if (!binding) return null;
  return canonicalizeDefinition({
    ...binding.release,
    selection: { alias: binding.alias, status: binding.status, catalog: binding.catalog },
  });
}
