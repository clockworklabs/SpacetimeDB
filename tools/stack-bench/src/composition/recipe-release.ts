import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';

import { compilePromotionFile, compileRecipeFile } from './composition-compiler.js';
import { compileScenarioDefinition, compileTrackManifest } from './definition-compiler.js';
import { canonicalDefinitionJson, canonicalizeDefinition } from './definition-plan.js';
import { sha256 } from '../evidence/provenance.js';
import { suitesFor } from './tracks.js';
import type {
  CompiledOwnedTaskFragment,
  CompiledPromotionCatalog,
  CompiledRecipePlan,
} from './composition-compiler.js';
import type { CompiledStep } from './definition-compiler.js';
import type { Track, TrackSuiteSource } from './tracks.js';

export const RECIPE_RELEASE_SCHEMA_VERSION = 1;

export interface RecipeCheck {
  stableKey: string;
  executionId: string;
  points: number;
  packId?: string;
  stablePackId?: string;
  packVersion?: string;
  checkGroupId?: string;
  role?: string;
  observations?: string[];
  requiresFeatures?: string[];
  source?: string;
  featureId?: number;
  criterionId?: string;
  description?: string;
  treatment?: string;
}

export interface RecipeTaskFragment {
  id: string;
  path: string;
  order: number;
  from: string | null;
  until: string | null;
  owners: string[];
  ownerConditions?: Array<{ owner: string; modes: string[]; requiresFeatures: string[] }>;
  modes: string[];
  requiresFeatures?: string[];
  sha256: string;
}

export interface RecipePackComponent {
  id: string;
  version: string;
  state: string;
  path: string;
  sha256: string;
  includeRoles: string[];
  stableId?: string;
  moduleType?: string;
  requiresPacks: string[];
}

export interface RecipeReleaseIdentity extends Record<string, unknown> {
  recipeReleaseSchemaVersion: number;
  id: string;
  version: string;
  state: string;
  track: string;
  meaningSha256: string;
  executionSha256: string;
  contentSha256: string;
  sourceManifestSha256: string;
}

interface RecipeSourceManifestEntry {
  path: string;
  sha256: string;
  kinds: string[];
}

export interface RecipeRelease extends RecipeReleaseIdentity {
  title: string;
  compatibility: { legacyLevel?: number; mode?: string } | null;
  capabilities: string[];
  scoring: { mode: string; checks: number; points: number };
  checkCatalog: RecipeCheck[];
  sourceManifest: RecipeSourceManifestEntry[];
  components: {
    fixture: { id: string; version: string; state: string; path: string; sha256: string };
    packs: RecipePackComponent[];
  };
  task: {
    mode: string;
    baseRecipe: RecipeReleaseIdentity | null;
    requirements: RecipeTaskFragment[];
    contracts: RecipeTaskFragment[];
    requirementSha256: string;
    contractSha256: string;
    composedSha256: string;
  };
}

export type RecipeExecutionOwnership =
  | { kind: 'current'; level?: number }
  | { kind: 'inherited'; fromLevel?: number };

export interface RecipeExecution {
  id: string;
  source?: string;
  ownership: RecipeExecutionOwnership;
}

interface RecipeCatalogIdentity {
  id: string;
  version: string;
  state: string;
  title: string;
  path: string;
  sha256: string;
}

export interface RecipeBinding {
  alias: string;
  status: string;
  catalog: RecipeCatalogIdentity;
  recipePath: string;
  plan: CompiledRecipePlan;
  release: RecipeRelease;
  execution: RecipeExecution[];
}

export type ExactRecipeRequest = string | {
  id: string;
  version: string;
  contentSha256?: string;
};

interface RecipeTaskDocuments {
  requirements: Array<CompiledOwnedTaskFragment & { sha256: string }>;
  contracts: Array<CompiledOwnedTaskFragment & { sha256: string }>;
  requirementText: string;
  contractText: string;
}

interface RecipeCheckDetail extends RecipeCheck {
  packId: string;
  packVersion: string;
  checkGroupId: string;
  role: string;
  source: string;
  featureId: number;
  criterionId: string;
  description: string;
  featureName: string;
  note: unknown;
  statedBy: unknown;
  provenBy: unknown;
  withheld: unknown;
  sourcePoints: number;
  semantics: unknown[];
}

interface RecipeProjectionFeature {
  id: number;
  criteria: Array<{ id: string; points: number }>;
}

interface RecipeProjectionExecution {
  id: string;
  source: string;
  features: RecipeProjectionFeature[];
}

interface ResolvedExactRecipe {
  id: string;
  version: string;
  contentSha256?: string;
}

export interface RecipeGradeRelease extends RecipeReleaseIdentity {
  selection: { alias: string; status: string; catalog: RecipeCatalogIdentity };
  executionId: string;
  checks: RecipeCheck[];
}

export interface BundledRecipeRelease extends RecipeRelease {
  selection: { alias: string; status: string; catalog: RecipeCatalogIdentity };
}

type UnknownRecord = Record<string, unknown>;

function record(value: unknown): value is UnknownRecord {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function recipeFixturePath(value: unknown, source: string): string {
  if (!record(value) || !record(value.fixture) || typeof value.fixture.path !== 'string') {
    throw new Error(`recipe ${source} has no fixture path`);
  }
  return value.fixture.path;
}

function assertRecipeRelease(value: unknown): asserts value is RecipeRelease {
  if (!record(value)
    || typeof value.recipeReleaseSchemaVersion !== 'number'
    || typeof value.id !== 'string'
    || typeof value.version !== 'string'
    || typeof value.state !== 'string'
    || typeof value.title !== 'string'
    || typeof value.track !== 'string'
    || typeof value.meaningSha256 !== 'string'
    || typeof value.executionSha256 !== 'string'
    || typeof value.contentSha256 !== 'string'
    || typeof value.sourceManifestSha256 !== 'string'
    || !Array.isArray(value.capabilities)
    || !Array.isArray(value.checkCatalog)
    || !Array.isArray(value.sourceManifest)
    || !record(value.scoring)
    || !record(value.components)
    || !record(value.components.fixture)
    || !Array.isArray(value.components.packs)
    || !record(value.task)
    || !Array.isArray(value.task.requirements)
    || !Array.isArray(value.task.contracts)) {
    throw new Error('compiled recipe release is not valid');
  }
}

function assertRecipeGradeRelease(value: unknown): asserts value is RecipeGradeRelease {
  if (!record(value) || !record(value.selection) || typeof value.executionId !== 'string'
    || !Array.isArray(value.checks) || typeof value.contentSha256 !== 'string') {
    throw new Error('compiled grade recipe release is not valid');
  }
}

function assertBundledRecipeRelease(value: unknown): asserts value is BundledRecipeRelease {
  if (!record(value) || !record(value.selection)) {
    throw new Error('bundled recipe release has no selection');
  }
  assertRecipeRelease(value);
}

function sortedTrackActions(value: unknown): Array<UnknownRecord & { id: string }> {
  if (!Array.isArray(value) || value.some(action => !record(action) || typeof action.id !== 'string')) {
    throw new Error('track manifest actions must have string ids');
  }
  return value.sort((left, right) => left.id.localeCompare(right.id));
}


const SHA256 = /^[a-f0-9]{64}$/;
const EXECUTION_ONLY_STEP_FIELDS = new Set([
  'actor', 'actors', 'testid', 'in', 'within', 'settleMs', 'ms', 'intervalMs',
  'delayMs', 'readyTestid', 'samples', 'secondsAhead',
]);
const ASSERTION_CONTAINS_ACTIONS = new Set([
  'expect', 'expectActorsWith', 'expectElementCount', 'expectNotReceived', 'expectReceived',
  'expectUnavailable',
]);

function readJson(path: string, label: string): unknown {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    throw new Error(`cannot read ${label} ${path}: ${message}`, { cause: error });
  }
}

function trackRelative(trackRoot: string, path: string): string {
  return relative(resolve(trackRoot), realpathSync(path)).replaceAll('\\', '/');
}

function semanticStep(step: CompiledStep): unknown {
  if (step.do === 'race') {
    return {
      do: step.do,
      branches: Array.isArray(step.branches)
        ? step.branches.map(branch => Array.isArray(branch) ? branch.map(semanticStep) : [])
        : [],
    };
  }
  return Object.fromEntries(Object.entries(step)
    .filter(([key]) => !EXECUTION_ONLY_STEP_FIELDS.has(key)
      && (key !== 'contains' || ASSERTION_CONTAINS_ACTIONS.has(step.do)))
    .map(([key, value]) => [key, canonicalizeDefinition(value)]));
}

function taskDocuments(plan: CompiledRecipePlan, trackRoot: string): RecipeTaskDocuments {
  const compact = (fragment: CompiledOwnedTaskFragment) => ({
    id: fragment.id,
    path: fragment.path,
    order: fragment.order,
    from: fragment.from,
    until: fragment.until,
    modes: fragment.modes,
    owners: fragment.owners,
    ...(fragment.ownerConditions === undefined
      ? {} : { ownerConditions: fragment.ownerConditions }),
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

function checkDetails(plan: CompiledRecipePlan): RecipeCheckDetail[] {
  const details: RecipeCheckDetail[] = [];
  for (const execution of plan.execution) {
    for (const group of execution.checkGroups) {
      for (const criterion of group.feature.criteria) {
        const stableKey = `${group.stablePackId ?? group.packId}.${group.checkGroupId}.${criterion.id}`;
        const compiled = plan.checks.find(check => check.stableKey === stableKey);
        if (!compiled) throw new Error(`compiled recipe lost check ${stableKey}`);
        details.push({
          stableKey,
          executionId: execution.id,
          packId: group.packId,
          ...(group.stablePackId === undefined ? {} : { stablePackId: group.stablePackId }),
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

function sourceEntry(trackRoot: string, path: string, kind: string): RecipeSourceManifestEntry {
  const absolute = realpathSync(path);
  return { path: trackRelative(trackRoot, absolute), sha256: sha256(readFileSync(absolute)), kinds: [kind] };
}

function mergeSources(entries: RecipeSourceManifestEntry[]): RecipeSourceManifestEntry[] {
  const merged = new Map<string, RecipeSourceManifestEntry>();
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

function releaseIdentity(release: RecipeRelease): RecipeReleaseIdentity {
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

export function recipeReleaseIdentity(release: RecipeRelease): RecipeReleaseIdentity {
  return canonicalizeDefinition(releaseIdentity(release)) as RecipeReleaseIdentity;
}

export function buildRecipeRelease(recipePath: string, {
  trackRoot,
}: { trackRoot?: string } = {}): RecipeRelease {
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
      requirements: documents.requirements.map(({ id, owners, ownerConditions,
        requiresFeatures, text }) => ({
        id, owners, ...(ownerConditions === undefined ? {} : { ownerConditions }),
        ...(requiresFeatures === undefined ? {} : { requiresFeatures }), text,
      })),
      contracts: documents.contracts.map(({ id, owners, ownerConditions,
        requiresFeatures, text }) => ({
        id, owners, ...(ownerConditions === undefined ? {} : { ownerConditions }),
        ...(requiresFeatures === undefined ? {} : { requiresFeatures }), text,
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
      ...(pack.stableId === undefined ? {} : { stableId: pack.stableId }),
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
      actions: sortedTrackActions(structuredClone(trackManifest.actions ?? [])),
    },
    execution: plan.execution.map(execution => ({
      id: execution.id,
      source: execution.source,
      scenario: scenarioDefinitions[execution.source],
      checkGroups: execution.checkGroups.map(group => ({
        packId: group.packId,
        ...(group.stablePackId === undefined ? {} : { stablePackId: group.stablePackId }),
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

  const fixturePath = realpathSync(resolve(dirname(absoluteRecipe), recipeFixturePath(rawRecipe, absoluteRecipe)));
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
  const packSource = (pack: CompiledRecipePlan['packs'][number]) => {
    const path = trackRelative(root, join(root, 'composition', pack.path));
    const source = sourceManifest.find(entry => entry.path === path && entry.kinds.includes('pack'));
    if (!source) throw new Error(`recipe release lost pack source ${path}`);
    return { path, sha256: source.sha256 };
  };

  const release = canonicalizeDefinition({
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
        ...(pack.stableId === undefined ? {} : { stableId: pack.stableId }),
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
      ...(detail.stablePackId === undefined ? {} : { stablePackId: detail.stablePackId }),
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
  assertRecipeRelease(release);
  return release;
}

function legacyProjection(track: Track, level: number): RecipeProjectionExecution[] {
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

function recipeProjection(plan: CompiledRecipePlan): RecipeProjectionExecution[] {
  return plan.execution.map(execution => ({
    id: execution.id,
    source: execution.source,
    features: execution.checkGroups.reduce<RecipeProjectionFeature[]>((features, group) => {
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

export function assertLegacyRecipeParity(plan: CompiledRecipePlan, track: Track, level: number): void {
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

function assertCompatibilityCandidateContinuity(
  plan: CompiledRecipePlan,
  promotedPlan: CompiledRecipePlan,
  track: Track,
  level: number,
): void {
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
    if (!next) throw new Error(`${plan.recipe.id}@${plan.recipe.version} lost check ${stableKey}`);
    if (next.points < previous.points || (previous.points > 0 && next.points !== previous.points)) {
      throw new Error(`${plan.recipe.id}@${plan.recipe.version} changes the established score for ${stableKey}`);
    }
  }
}

function cumulativeBasePlan(plan: CompiledRecipePlan, track: Track, level: number): CompiledRecipePlan {
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

function executionStableKeys(execution: CompiledRecipePlan['execution'][number]): string[] {
  return execution.checkGroups.flatMap(group => group.feature.criteria.map(criterion =>
    `${group.stablePackId ?? group.packId}.${group.checkGroupId}.${criterion.id}`));
}

function legacyExecutionPlan(
  plan: CompiledRecipePlan,
  track: TrackSuiteSource,
  level: number,
): RecipeExecution[] {
  const declared = new Map(suitesFor(track, level).map(suite => [suite.id, suite]));
  return plan.execution.map(execution => {
    const suite = declared.get(execution.id);
    if (!suite) {
      throw new Error(`${plan.recipe.id}@${plan.recipe.version} has no declared L${level} execution ${execution.id}`);
    }
    return {
      id: execution.id,
      source: execution.source,
      ownership: suite.inherited === true && typeof suite.fromLevel === 'number'
        ? { kind: 'inherited' as const, fromLevel: suite.fromLevel }
        : { kind: 'current' as const, level: Number(level) },
    };
  });
}

// Ownership is release structure, not an execution-id naming convention. A
// cumulative recipe inherits every check selected by its exact base recipe;
// the remaining checks belong to the level being introduced. Recursing through
// bases preserves the original owner when L3 carries both L1 and L2 checks.
function cumulativeExecutionPlan(
  plan: CompiledRecipePlan,
  track: TrackSuiteSource,
  level: number,
): RecipeExecution[] {
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
  const baseOrigins = new Map<string, number>();
  for (const owned of baseExecution) {
    const execution = baseById.get(owned.id);
    if (!execution) throw new Error(`base recipe lost execution ${owned.id}`);
    const origin = owned.ownership.kind === 'inherited'
      ? owned.ownership.fromLevel : owned.ownership.level;
    if (origin === undefined) throw new Error(`base recipe execution ${owned.id} has no owner level`);
    for (const stableKey of executionStableKeys(execution)) baseOrigins.set(stableKey, origin);
  }

  return plan.execution.map(execution => {
    const stableKeys = executionStableKeys(execution);
    const inherited = stableKeys.filter(stableKey => baseOrigins.has(stableKey));
    if (inherited.length === 0) {
      return { id: execution.id, source: execution.source,
        ownership: { kind: 'current' as const, level: Number(level) } };
    }
    if (inherited.length !== stableKeys.length) {
      throw new Error(`${plan.recipe.id}@${plan.recipe.version} execution ${execution.id} `
        + 'mixes inherited and current-level checks');
    }
    const origins = new Set<number>();
    for (const stableKey of inherited) {
      const origin = baseOrigins.get(stableKey);
      if (origin === undefined) throw new Error(`base recipe lost check owner ${stableKey}`);
      origins.add(origin);
    }
    if (origins.size !== 1) {
      throw new Error(`${plan.recipe.id}@${plan.recipe.version} execution ${execution.id} `
        + `mixes checks owned by levels ${[...origins].sort((a, b) => a - b).join(', ')}`);
    }
    return { id: execution.id, source: execution.source,
      ownership: { kind: 'inherited' as const, fromLevel: [...origins][0] ?? baseLevel } };
  });
}

export function executionPlanForRecipe(
  plan: CompiledRecipePlan,
  track: TrackSuiteSource,
  level: number,
): RecipeExecution[] {
  if (plan.recipe.compatibility?.mode === 'cumulative') {
    return cumulativeExecutionPlan(plan, track, level);
  }
  if (plan.recipe.compatibility !== null) return legacyExecutionPlan(plan, track, level);
  return plan.execution.map(execution => ({
    id: execution.id,
    source: execution.source,
    ownership: { kind: 'current' as const, level: Number(level) },
  }));
}

export function executionPlanForRelease(recipePath: string, {
  trackRoot,
  level,
}: { trackRoot: string; level: number }): RecipeExecution[] {
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

function assertInitialCumulativeBase(
  plan: CompiledRecipePlan,
  promotionCatalog: CompiledPromotionCatalog,
  track: Track,
  level: number,
): void {
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
  if (!selected) throw new Error(`${lowerAlias} promoted base selection disappeared`);
  if (embedded.recipe.id !== selected.recipe.id || embedded.recipe.version !== selected.recipe.version) {
    throw new Error(`${plan.recipe.id}@${plan.recipe.version} initial cumulative L${numericLevel} base `
      + `${embedded.recipe.id}@${embedded.recipe.version} is not promoted ${lowerAlias} `
      + `${selected.recipe.id}@${selected.recipe.version}`);
  }
  const baseRecipe = plan.recipe.task.baseRecipe;
  if (!baseRecipe) throw new Error(`${plan.recipe.id}@${plan.recipe.version} lost its base recipe`);
  const embeddedRelease = buildRecipeRelease(join(track.dir, 'composition', baseRecipe.path), {
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

function assertCumulativeContinuity(
  plan: CompiledRecipePlan,
  previousPlans: CompiledRecipePlan[],
  track: Track,
  level: number,
): void {
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
    if (!next) throw new Error(`${plan.recipe.id}@${plan.recipe.version} lost check ${previous.stableKey}`);
    if (next.points < previous.points || (previous.points > 0 && next.points !== previous.points)) {
      throw new Error(`${plan.recipe.id}@${plan.recipe.version} changes the established score for ${previous.stableKey}`);
    }
  }
}

function exactRecipeRequest(requested: ExactRecipeRequest | null | undefined): ResolvedExactRecipe | null {
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
  if (requested.contentSha256 !== undefined && !SHA256.test(requested.contentSha256)) {
    throw new Error('exact recipe selection contentSha256 must be a SHA-256 digest');
  }
  return { id: requested.id, version: requested.version,
    ...(requested.contentSha256 !== undefined ? { contentSha256: requested.contentSha256 } : {}) };
}

// Normal runs resolve the public promoted L<n> alias. An exact request may use
// a non-default release from the separate catalog before or after promotion.
// Both choices return the same binding and use the same runner path.
// TODO(ts-migration): this overload hides the null return below; correcting it
// touches ~60 callers and belongs with the test migration.
export function resolveRecipeRelease(
  track: Track,
  level: number,
  requested?: ExactRecipeRequest | null,
): RecipeBinding;
export function resolveRecipeRelease(
  track: Track,
  level: number,
  requested: ExactRecipeRequest | null = null,
): RecipeBinding | null {
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
      && entry.status !== 'retired');
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
  if (!selection) throw new Error(`${alias} recipe selection disappeared`);
  const recipePath = join(track.dir, 'composition', selection.recipe.path);
  const plan = compileRecipeFile(recipePath, { trackRoot: track.dir });
  const compatibilityMode = plan.recipe.compatibility?.mode ?? 'legacy-parity';
  if (plan.recipe.compatibility !== null && selection.status === 'candidate') {
    if (compatibilityMode === 'cumulative') {
      if (promoted.length === 0) assertInitialCumulativeBase(plan, promotionCatalog, track, level);
      else {
        const promotedEntry = promoted[0];
        if (!promotedEntry) throw new Error(`${alias} promoted recipe selection disappeared`);
        const promotedPlan = compileRecipeFile(join(track.dir, 'composition', promotedEntry.recipe.path),
          { trackRoot: track.dir });
        assertCumulativeContinuity(plan, [promotedPlan], track, level);
      }
    } else {
      if (promoted.length !== 1) {
        throw new Error(`${alias} compatibility candidate requires exactly one promoted baseline; found ${promoted.length}`);
      }
      const promotedEntry = promoted[0];
      if (!promotedEntry) throw new Error(`${alias} promoted recipe selection disappeared`);
      const promotedPlan = compileRecipeFile(join(track.dir, 'composition', promotedEntry.recipe.path),
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
    || plan.packs.some(pack => pack.moduleType === undefined
      || !['feature', 'specification'].includes(pack.moduleType))) {
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

export function gradeRecipeRelease(
  binding: RecipeBinding | null,
  executionId: string,
  featureId: number | null = null,
): RecipeGradeRelease | null {
  if (!binding) return null;
  const checks = binding.release.checkCatalog.filter(check => check.executionId === executionId
    && (featureId === null || check.featureId === featureId));
  if (!checks.length) throw new Error(`recipe ${binding.release.id} has no execution ${executionId}`);
  const release = canonicalizeDefinition({
    ...recipeReleaseIdentity(binding.release),
    selection: { alias: binding.alias, status: binding.status, catalog: binding.catalog },
    executionId,
    checks,
  });
  assertRecipeGradeRelease(release);
  return release;
}

export function resolveGradeRecipeArtifactBinding(
  track: Track,
  level: number,
  specPath: string,
  featureId: number | null = null,
  requested: ExactRecipeRequest | null = null,
): { release: RecipeGradeRelease; sourceRelease: RecipeRelease } | null {
  const binding = resolveRecipeRelease(track, level, requested);
  if (!binding) return null;
  const absoluteSpec = realpathSync(specPath);
  const execution = binding.plan.execution.find(candidate =>
    realpathSync(join(track.dir, candidate.source)) === absoluteSpec);
  if (!execution) {
    throw new Error(`recipe ${binding.release.id}@${binding.release.version} does not select scenario ${specPath}`);
  }
  const gradeRelease = gradeRecipeRelease(binding, execution.id, featureId);
  if (!gradeRelease) throw new Error(`recipe ${binding.release.id} grade release disappeared`);
  return {
    release: gradeRelease,
    sourceRelease: binding.release,
  };
}

export function resolveGradeRecipeRelease(
  track: Track,
  level: number,
  specPath: string,
  featureId: number | null = null,
  requested: ExactRecipeRequest | null = null,
): RecipeGradeRelease | null {
  return resolveGradeRecipeArtifactBinding(track, level, specPath, featureId, requested)?.release ?? null;
}

export function bundleRecipeRelease(binding: RecipeBinding | null): BundledRecipeRelease | null {
  if (!binding) return null;
  const release = canonicalizeDefinition({
    ...binding.release,
    selection: { alias: binding.alias, status: binding.status, catalog: binding.catalog },
  });
  assertBundledRecipeRelease(release);
  return release;
}
