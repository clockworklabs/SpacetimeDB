import { z } from 'zod';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { buildRecipeQualificationDocuments } from './recipe-release.js';
import type { CalibrationPlan } from './calibration-compiler.js';
import { canonicalDefinitionJson } from './definition-plan.js';
import { sha256 } from '../evidence/provenance.js';
import { interfaceNeutralText } from './agent-visible-contract.js';
import type { RecipeCheck, RecipeRelease } from './recipe-release.js';

const object = z.record(z.string(), z.unknown());
const documentsSchema = z.object({
  release: object,
  meaning: object,
  execution: object,
});

const equal = (a: unknown, b: unknown) => a === undefined || b === undefined
  ? a === b : canonicalDefinitionJson(a) === canonicalDefinitionJson(b);
const digest = (value: unknown) => sha256(canonicalDefinitionJson(value));
function fail(message: string): never { throw new Error(`qualification slice: ${message}`); }
const records = (value: unknown): Record<string, unknown>[] => z.array(object).parse(value);

export interface QualificationDocuments {
  release: RecipeRelease;
  meaning: Record<string, unknown>;
  execution: Record<string, unknown>;
}

export function writeQualificationSnapshot(path: string, recipePath: string,
  calibration: CalibrationPlan, stackBenchRoot: string): void {
  const documents = buildRecipeQualificationDocuments(recipePath,
    { trackRoot: join(stackBenchRoot, 'tracks', calibration.track) });
  const mutations = Object.fromEntries(calibration.mutations.map(entry => [entry.backend,
    JSON.parse(readFileSync(resolve(stackBenchRoot, entry.path), 'utf8'))]));
  writeFileSync(path, JSON.stringify({ documents, calibration, mutations }) + '\n', { flag: 'wx' });
}

/** Validate the saved hash preimages, and derive check metadata from them.
 * The saved release's catalog is not accepted as an independent assertion. */
export function validateQualificationDocuments(value: unknown): QualificationDocuments {
  const parsed = documentsSchema.parse(value);
  const { meaning, execution } = parsed;
  const release = parsed.release as unknown as RecipeRelease;
  if (meaning.schemaVersion !== 3 || execution.schemaVersion !== 3
    || release.recipeReleaseSchemaVersion !== 3
    || meaning.track !== release.track || execution.track !== release.track
    || release.meaningSha256 !== digest(meaning) || release.executionSha256 !== digest(execution)
    || release.contentSha256 !== digest({ schemaVersion: 3,
      meaningSha256: release.meaningSha256, executionSha256: release.executionSha256 })) {
    fail('recipe hash inputs do not match the saved release');
  }
  const locations = new Map<string, string>();
  for (const entry of records(execution.execution)) {
    for (const group of records(entry.checkGroups)) {
      const feature = object.parse(group.feature);
      for (const criterion of records(feature.criteria)) {
        const key = `${group.stablePackId ?? group.packId}.${group.checkGroupId}.${criterion.id}`;
        if (locations.has(key) || typeof entry.id !== 'string') fail('ambiguous execution ownership');
        locations.set(key, entry.id);
      }
    }
  }
  const meanings = records(meaning.checks);
  if (!Array.isArray(release.checkCatalog) || release.checkCatalog.length !== meanings.length
    || locations.size !== meanings.length) fail('recipe catalog is incomplete');
  const seen = new Set<string>();
  for (const check of release.checkCatalog) {
    const source = meanings.find(item => item.stableKey === check.stableKey);
    if (!source || seen.has(check.stableKey) || locations.get(check.stableKey) !== check.executionId) {
      fail('recipe catalog has invalid check ownership');
    }
    seen.add(check.stableKey);
    for (const field of ['packId', 'checkGroupId', 'role', 'category', 'observations',
      'requiresFeatures', 'source', 'featureId', 'criterionId', 'description', 'points'] as const) {
      if (!equal(check[field], source[field])) fail(`catalog differs from hash inputs: ${check.stableKey}.${field}`);
    }
  }
  return { release, meaning, execution };
}

// Interface blocks belong to their stacks' scopes (qualification-scope.ts), and a
// saved document may predate interface-neutral meaning; compare the shared text only.
function interfaceNeutralMeaning(meaning: Record<string, unknown>): Record<string, unknown> {
  const parsed = z.object({ contracts: z.array(object) }).loose().safeParse(meaning.task);
  if (!parsed.success) return meaning;
  const task = parsed.data;
  return { ...meaning, task: { ...task,
    contracts: task.contracts.map(contract => ({ ...contract, text: interfaceNeutralText(contract.text) })) } };
}

/** Conservative reuse boundary: a complete scenario, including all its setup.
 * Changes to shared runtime, fixture, prompt or execution order
 * invalidate reuse. Scenarios are reset separately by run-suite. */
export function unchangedQualificationChecks(source: QualificationDocuments,
  current: QualificationDocuments): Set<string> {
  const { checks: _oldChecks, ...oldMeaning } = interfaceNeutralMeaning(source.meaning);
  const { checks: _newChecks, ...newMeaning } = interfaceNeutralMeaning(current.meaning);
  const { execution: oldExecutions, packs: oldPacks, ...oldShared } = source.execution;
  const { execution: newExecutions, packs: newPacks, ...newShared } = current.execution;
  // `actions` is the union inferred from the pack's scenarios. Actual steps
  // are compared below; adding an action in one scenario does not change a
  // sibling scenario. Keep budgets and every other pack input in the comparison.
  const packInputs = ({ actions: _actions, ...pack }: Record<string, unknown>) => pack;
  const oldPackMap = new Map(records(oldPacks).map(pack => [pack.id, packInputs(pack)]));
  const unchangedPacks = new Set(records(newPacks)
    .filter(pack => equal(packInputs(pack), oldPackMap.get(pack.id))).map(pack => pack.id));
  if (!equal(oldMeaning, newMeaning) || !equal(oldShared, newShared)
    || !equal(records(oldExecutions).map(({ id, source }) => ({ id, source })),
      records(newExecutions).map(({ id, source }) => ({ id, source })))) return new Set();
  const oldById = new Map(records(oldExecutions).map(entry => [entry.id, entry]));
  const unchangedExecutions = new Set(records(newExecutions)
    .filter(entry => equal(entry, oldById.get(entry.id))).map(entry => entry.id));
  const oldByKey = new Map(records(source.meaning.checks).map(check => [check.stableKey, check]));
  const newByKey = new Map(records(current.meaning.checks).map(check => [check.stableKey, check]));
  return new Set(current.release.checkCatalog.filter(check => unchangedExecutions.has(check.executionId)
    && unchangedPacks.has(check.packId)
    && equal(newByKey.get(check.stableKey), oldByKey.get(check.stableKey)))
    .map(check => check.stableKey));
}

export function assertQualificationSliceCoverage(entries: Array<{
  kind: string; stack?: string; repetition: number; checks: string[];
}>, required: string[], checks: RecipeCheck[]): void {
  const expectedChecks = new Set(checks.map(check => check.stableKey));
  const coverage = new Map(required.map(key => [key, new Set<string>()]));
  for (const entry of entries) {
    const key = `${entry.kind}:${entry.stack ?? ''}:${entry.repetition}`;
    const covered = coverage.get(key);
    if (!covered || entry.checks.length === 0) fail(`unexpected or empty evidence scope ${key}`);
    for (const check of entry.checks) {
      if (!expectedChecks.has(check)) fail(`unknown check ${check}`);
      if (covered.has(check)) fail(`duplicate coverage for ${key}:${check}`);
      covered.add(check);
    }
  }
  for (const [key, covered] of coverage) {
    const missing = [...expectedChecks].filter(check => !covered.has(check));
    if (missing.length) fail(`missing coverage for ${key}: ${missing.join(', ')}`);
  }
}
