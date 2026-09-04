import { existsSync, readFileSync, realpathSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { z } from 'zod';

import { compileRecipeSelectionFile, compileRecipeFile } from './composition-compiler.js';
import { compileScenarioDefinition, compileTrackManifest } from './definition-compiler.js';
import { canonicalDefinitionJson, canonicalizeDefinition, readDefinitionJson }
  from './definition-plan.js';
import { sha256 } from '../evidence/provenance.js';
import type {
  CompiledOwnedTaskFragment,
  CompiledRecipeSelectionCatalog,
  CompiledRecipePlan,
} from './composition-compiler.js';
import type { CompiledStep } from './definition-compiler.js';
import { TRACK_MANIFEST_FILE, type Track } from './tracks.js';

export const RECIPE_RELEASE_SCHEMA_VERSION = 3;

export interface RecipeCheck {
  stableKey: string;
  executionId: string;
  points: number;
  packId?: string;
  stablePackId?: string;
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
  stableId?: string;
  path: string;
  sha256: string;
  includeRoles: string[];
  includeCheckGroups?: string[];
  moduleType?: string;
  requiresPacks: string[];
}

export interface RecipeReleaseIdentity extends Record<string, unknown> {
  recipeReleaseSchemaVersion: number;
  id: string;
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
  sequence: { level: number } | null;
  capabilities: string[];
  scoring: { mode: string; checks: number; points: number };
  checkCatalog: RecipeCheck[];
  sourceManifest: RecipeSourceManifestEntry[];
  components: {
    fixture: { id: string; path: string; sha256: string };
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

interface RecipeSelectionIdentity {
  path: string;
  sha256: string;
}

export interface RecipeBinding {
  alias: string;
  selection: RecipeSelectionIdentity;
  recipePath: string;
  plan: CompiledRecipePlan;
  release: RecipeRelease;
  execution: RecipeExecution[];
}

export type RecipeRequest = string | {
  id: string;
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

interface ResolvedRecipe {
  id: string;
  contentSha256?: string;
}

export interface RecipeGradeRelease extends RecipeReleaseIdentity {
  selection: { alias: string; selection: RecipeSelectionIdentity };
  executionId: string;
  checks: RecipeCheck[];
}

export interface BundledRecipeRelease extends RecipeRelease {
  selection: { alias: string; selection: RecipeSelectionIdentity };
}

type UnknownRecord = Record<string, unknown>;

const recipeReleaseSchema = z.looseObject({
  recipeReleaseSchemaVersion: z.number(),
  id: z.string(),
  title: z.string(),
  track: z.string(),
  meaningSha256: z.string(),
  executionSha256: z.string(),
  contentSha256: z.string(),
  sourceManifestSha256: z.string(),
  capabilities: z.array(z.unknown()),
  checkCatalog: z.array(z.unknown()),
  sourceManifest: z.array(z.unknown()),
  scoring: z.looseObject({}),
  components: z.looseObject({ fixture: z.looseObject({}), packs: z.array(z.unknown()) }),
  task: z.looseObject({ requirements: z.array(z.unknown()), contracts: z.array(z.unknown()) }),
});
const gradeRecipeReleaseSchema = z.looseObject({
  selection: z.looseObject({}),
  executionId: z.string(),
  checks: z.array(z.unknown()),
  contentSha256: z.string(),
});
const bundledRecipeReleaseSchema = z.looseObject({ selection: z.looseObject({}) });

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
  if (!recipeReleaseSchema.safeParse(value).success) {
    throw new Error('compiled recipe release is not valid');
  }
}

function assertRecipeGradeRelease(value: unknown): asserts value is RecipeGradeRelease {
  if (!gradeRecipeReleaseSchema.safeParse(value).success) {
    throw new Error('compiled grade recipe release is not valid');
  }
}

function assertBundledRecipeRelease(value: unknown): asserts value is BundledRecipeRelease {
  if (!bundledRecipeReleaseSchema.safeParse(value).success) {
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

function taskDocuments(plan: CompiledRecipePlan): RecipeTaskDocuments {
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
  const rawRecipe = readDefinitionJson(absoluteRecipe, 'recipe');
  const trackManifestPath = join(root, TRACK_MANIFEST_FILE);
  const trackManifest = compileTrackManifest(
    readDefinitionJson(trackManifestPath, 'track manifest'), {
    source: trackManifestPath,
  });
  const documents = taskDocuments(plan);
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
    const scenario = compileScenarioDefinition(readDefinitionJson(path, 'scenario'),
      { source: path });
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
      ...(pack.includeCheckGroups === undefined
        ? {} : { includeCheckGroups: [...pack.includeCheckGroups].sort() }),
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
    title: plan.recipe.title,
    track: plan.recipe.track,
    sequence: plan.recipe.sequence,
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
      fixture: { id: plan.fixture.id,
        path: trackRelative(root, fixturePath), sha256: sha256(readFileSync(fixturePath)) },
      packs: plan.packs.map(pack => ({ id: pack.id,
        ...packSource(pack), includeRoles: [...pack.includeRoles].sort(),
        ...(pack.includeCheckGroups === undefined
          ? {} : { includeCheckGroups: [...pack.includeCheckGroups].sort() }),
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

function sequentialBasePlan(plan: CompiledRecipePlan, track: { dir: string }, level: number): CompiledRecipePlan {
  if (plan.recipe.sequence?.level !== Number(level) || Number(level) <= 1) {
    throw new Error(`${plan.recipe.id} is not sequential L${level}`);
  }
  const base = plan.recipe.task.baseRecipe;
  if (!base) throw new Error(`${plan.recipe.id} has no sequential base recipe`);
  const basePlan = compileRecipeFile(join(track.dir, 'composition', base.path), {
    trackRoot: track.dir,
  });
  const candidateByKey = new Map(plan.checks.map(check => [check.stableKey, check]));
  for (const check of basePlan.checks) {
    const carried = candidateByKey.get(check.stableKey);
    if (!carried || canonicalDefinitionJson(carried) !== canonicalDefinitionJson(check)) {
      throw new Error(`${plan.recipe.id} does not carry base check ${check.stableKey} exactly`);
    }
  }
  return basePlan;
}

function executionStableKeys(execution: CompiledRecipePlan['execution'][number]): string[] {
  return execution.checkGroups.flatMap(group => group.feature.criteria.map(criterion =>
    `${group.stablePackId ?? group.packId}.${group.checkGroupId}.${criterion.id}`));
}

// Preserve check ownership through the exact base-recipe chain.
function sequentialUpgradeExecutionPlan(
  plan: CompiledRecipePlan,
  track: { dir: string },
  level: number,
): RecipeExecution[] {
  const base = plan.recipe.task.baseRecipe;
  if (!base) throw new Error(`${plan.recipe.id} has no sequential base recipe`);
  const basePlan = compileRecipeFile(join(track.dir, 'composition', base.path), {
    trackRoot: track.dir,
  });
  const baseLevel = basePlan.recipe.sequence?.level;
  if (baseLevel === undefined || !Number.isInteger(baseLevel)
      || baseLevel < 1 || baseLevel !== Number(level) - 1) {
    throw new Error(`${plan.recipe.id} must inherit exact L${Number(level) - 1}, `
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
      throw new Error(`${plan.recipe.id} execution ${execution.id} `
        + 'mixes inherited and current-level checks');
    }
    const origins = new Set<number>();
    for (const stableKey of inherited) {
      const origin = baseOrigins.get(stableKey);
      if (origin === undefined) throw new Error(`base recipe lost check owner ${stableKey}`);
      origins.add(origin);
    }
    if (origins.size !== 1) {
      throw new Error(`${plan.recipe.id} execution ${execution.id} `
        + `mixes checks owned by levels ${[...origins].sort((a, b) => a - b).join(', ')}`);
    }
    return { id: execution.id, source: execution.source,
      ownership: { kind: 'inherited' as const, fromLevel: [...origins][0] ?? baseLevel } };
  });
}

export function executionPlanForRecipe(
  plan: CompiledRecipePlan,
  track: { dir: string },
  level: number,
): RecipeExecution[] {
  const sequenceLevel = plan.recipe.sequence?.level;
  if (sequenceLevel !== undefined && sequenceLevel !== Number(level)) {
    throw new Error(`${plan.recipe.id} is sequential L${sequenceLevel}, not L${level}`);
  }
  if (sequenceLevel !== undefined && sequenceLevel > 1) {
    return sequentialUpgradeExecutionPlan(plan, track, level);
  }
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
  return executionPlanForRecipe(plan, { dir: root }, Number(level));
}

function assertSequentialBase(plan: CompiledRecipePlan, selection: CompiledRecipeSelectionCatalog,
  track: Track, level: number): void {
  if (Number(level) <= 1) return;
  const base = plan.recipe.task.baseRecipe;
  const alias = `L${Number(level) - 1}`;
  const selected = selection.entries.filter(entry => entry.alias === alias);
  if (!base || selected.length !== 1) throw new Error(`${plan.recipe.id} requires one ${alias} base recipe`);
  sequentialBasePlan(plan, track, level);
  const current = selected[0];
  if (!current || base.id !== current.recipe.id) {
    throw new Error(`${plan.recipe.id} does not select the current ${alias} recipe`);
  }
  const embedded = buildRecipeRelease(join(track.dir, 'composition', base.path), { trackRoot: track.dir });
  const resolved = buildRecipeRelease(join(track.dir, 'composition', current.recipe.path), { trackRoot: track.dir });
  if (embedded.contentSha256 !== resolved.contentSha256) {
    throw new Error(`${plan.recipe.id} does not bind the current ${alias} content`);
  }
}

export function validateRecipeRequest(requested: unknown): ResolvedRecipe | null {
  if (requested === null || requested === undefined) return null;
  if (typeof requested === 'string') return { id: requested };
  if (!record(requested) || typeof requested.id !== 'string') {
    throw new Error('recipe selection requires an id');
  }
  if (Object.keys(requested).some(field => !new Set(['id', 'contentSha256']).has(field))) {
    throw new Error('recipe selection contains an unknown field');
  }
  const contentSha256 = requested.contentSha256;
  if (contentSha256 !== undefined && (typeof contentSha256 !== 'string' || !SHA256.test(contentSha256))) {
    throw new Error('recipe selection contentSha256 must be a SHA-256 digest');
  }
  return { id: requested.id, ...(contentSha256 === undefined ? {} : { contentSha256 }) };
}

export function resolveRecipeRelease(track: Track, level: number,
  requested: RecipeRequest | null = null): RecipeBinding | null {
  const alias = `L${Number(level)}`;
  const exact = validateRecipeRequest(requested);
  const modes = exact ? ['sequential', 'dependency'] as const : ['sequential'] as const;
  const choices = modes.flatMap(selectionMode => {
    const path = join(track.dir, 'composition', `${selectionMode}.json`);
    if (!existsSync(path)) return [];
    const catalog = compileRecipeSelectionFile(path, { trackRoot: track.dir });
    return catalog.entries.filter(entry => entry.alias === alias
      && (!exact || entry.recipe.id === exact.id)).map(entry => ({ catalog, entry, path }));
  });
  if (!choices.length && !exact) return null;
  if (choices.length !== 1) throw new Error(`${alias} requires exactly one selected recipe; found ${choices.length}`);
  const choice = choices[0];
  if (!choice) throw new Error(`${alias} recipe selection disappeared`);
  const { catalog, entry: selected, path: selectionPath } = choice;
  const recipePath = join(track.dir, 'composition', selected.recipe.path);
  const plan = compileRecipeFile(recipePath, { trackRoot: track.dir });
  if (plan.recipe.sequence?.level !== undefined && plan.recipe.sequence.level !== Number(level)) {
    throw new Error(`${plan.recipe.id} is sequential L${plan.recipe.sequence.level}, not ${alias}`);
  }
  if (plan.recipe.sequence?.level && plan.recipe.sequence.level > 1) {
    assertSequentialBase(plan, catalog, track, level);
  }
  const release = buildRecipeRelease(recipePath, { trackRoot: track.dir });
  if (exact?.contentSha256 && release.contentSha256 !== exact.contentSha256) {
    throw new Error(`${exact.id} content changed: expected ${exact.contentSha256}, resolved ${release.contentSha256}`);
  }
  return { alias, selection: { path: trackRelative(track.dir, selectionPath),
    sha256: sha256(readFileSync(selectionPath)) }, recipePath, plan,
    execution: executionPlanForRecipe(plan, track, level), release };
}

export function requireRecipeRelease(
  track: Track,
  level: number,
  requested: RecipeRequest | null = null,
): RecipeBinding {
  const binding = resolveRecipeRelease(track, level, requested);
  if (!binding) {
    throw new Error(`L${level} has no recipe release`);
  }
  return binding;
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
    selection: { alias: binding.alias, selection: binding.selection },
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
  requested: RecipeRequest | null = null,
): { release: RecipeGradeRelease; sourceRelease: RecipeRelease } | null {
  const binding = resolveRecipeRelease(track, level, requested);
  if (!binding) return null;
  const absoluteSpec = realpathSync(specPath);
  const execution = binding.plan.execution.find(candidate =>
    realpathSync(join(track.dir, candidate.source)) === absoluteSpec);
  if (!execution) {
    throw new Error(`recipe ${binding.release.id} does not select scenario ${specPath}`);
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
  requested: RecipeRequest | null = null,
): RecipeGradeRelease | null {
  return resolveGradeRecipeArtifactBinding(track, level, specPath, featureId, requested)?.release ?? null;
}

export function bundleRecipeRelease(binding: RecipeBinding | null): BundledRecipeRelease | null {
  if (!binding) return null;
  const release = canonicalizeDefinition({
    ...binding.release,
    selection: { alias: binding.alias, selection: binding.selection },
  });
  assertBundledRecipeRelease(release);
  return release;
}
