import { existsSync, readFileSync } from 'node:fs';
import { basename } from 'node:path';

import {
  mutationFileEdits,
  mutationTargetKeys,
  resolveMutationFile,
  validateMutationDefinitions,
} from './mutation-analysis.js';
import type { MutationDefinition } from './mutation-analysis.js';
import type { RecipeCheck, RecipeRelease } from '../composition/recipe-release.mjs';

type BlockReason = 'unsafe-file' | 'missing-file' | 'anchor-mismatch';

interface MutationManifest {
  schemaVersion?: unknown;
  level?: unknown;
  mutations?: MutationDefinition[];
  track?: unknown;
  backend?: unknown;
  scenario?: string | null;
}

interface RebaseOptions {
  release?: RecipeRelease;
  selectedCheckKeys?: string[];
  app?: string;
  fixtureSha256?: string;
  note?: string;
}

interface AnchorIssue {
  reason: BlockReason;
  detail: string;
}

interface BlockedMutation {
  id: unknown;
  reason: BlockReason | 'unknown-target';
  detail?: string;
  targets?: string[];
}

interface ExcludedMutation {
  id: unknown;
  targets: string[];
}

interface RebasedMutation extends MutationDefinition {
  id: string;
  scenario: string;
  targets: string[];
}

export interface MutationRebaseResult {
  manifest: {
    schemaVersion: 2;
    status: 'candidate';
    fixtureSha256: string;
    backend: unknown;
    track: string;
    note?: string;
    mutations: RebasedMutation[];
  };
  blocked: BlockedMutation[];
  excluded: ExcludedMutation[];
  coverage: {
    selected: string[];
    covered: string[];
    missing: string[];
  };
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function scenarioPath(track: string, source: string): string {
  return `tracks/${track}/${source}`;
}

function scenarioSuffix(source: string): string {
  return basename(source, '.json')
    .replace(/^progression-/, '')
    .replace(/-\d+\.\d+\.\d+$/, '');
}

function anchorIssue(app: string, mutation: MutationDefinition): AnchorIssue | null {
  for (const edit of mutationFileEdits(mutation)) {
    let file: string;
    try {
      file = resolveMutationFile(app, edit.file);
    } catch (error) {
      return { reason: 'unsafe-file', detail: errorMessage(error) };
    }
    if (!existsSync(file)) {
      return { reason: 'missing-file', detail: edit.file };
    }
    const matches = readFileSync(file, 'utf8').split(edit.find).length - 1;
    if (matches !== 1) {
      return { reason: 'anchor-mismatch', detail: `${edit.file} matched ${matches} times` };
    }
  }
  return null;
}

function selectedKeys(
  release: RecipeRelease | undefined,
  keys: string[] | undefined,
): { catalog: Map<string, RecipeCheck>; selected: Set<string>; release: RecipeRelease } {
  if (!release?.track || !Array.isArray(release.checkCatalog)) {
    throw new Error('mutation rebase requires a compiled recipe release');
  }
  if (!Array.isArray(keys) || keys.length === 0 || new Set(keys).size !== keys.length) {
    throw new Error('mutation rebase requires unique selected check keys');
  }
  const catalog = new Map(release.checkCatalog.map(check => [check.stableKey, check]));
  const unknown = keys.filter(key => !catalog.has(key));
  if (unknown.length) {
    throw new Error(`mutation rebase selected unknown checks: ${unknown.sort().join(', ')}`);
  }
  return { catalog, selected: new Set(keys), release };
}

export function rebaseMutationManifest(
  manifest: MutationManifest | null | undefined,
  { release, selectedCheckKeys, app, fixtureSha256, note }: RebaseOptions = {},
): MutationRebaseResult {
  if (manifest?.schemaVersion !== 2 || manifest.level !== undefined) {
    throw new Error('mutation rebase accepts only level-independent schema 2 manifests');
  }
  if (!Array.isArray(manifest.mutations) || manifest.mutations.length === 0) {
    throw new Error('mutation rebase requires a non-empty mutation manifest');
  }
  if (
    typeof app !== 'string'
    || !app
    || typeof fixtureSha256 !== 'string'
    || !/^[a-f0-9]{64}$/.test(fixtureSha256)
  ) {
    throw new Error('mutation rebase requires an app directory and fixture SHA-256');
  }
  if (note !== undefined && (typeof note !== 'string' || !note.trim())) {
    throw new Error('mutation rebase note must be a non-empty string');
  }
  if (manifest.track !== release?.track) {
    throw new Error('mutation manifest and recipe release target different tracks');
  }
  const definitions = validateMutationDefinitions(manifest.mutations, {
    defaultScenario: manifest.scenario,
    requireScenario: true,
  });
  if (!definitions.ok) {
    throw new Error(`invalid mutation definitions: ${definitions.issues
      .map(issue => `${issue.mutation ?? '<unnamed>'}:${issue.kind}`).join(', ')}`);
  }

  const { catalog, selected, release: compiledRelease } = selectedKeys(release, selectedCheckKeys);
  const blocked: BlockedMutation[] = [];
  const excluded: ExcludedMutation[] = [];
  const mutations: RebasedMutation[] = [];
  for (const mutation of manifest.mutations) {
    const mutationId = mutation.id as string;
    const targets = mutationTargetKeys(mutation);
    const unknown = targets.filter(key => !catalog.has(key));
    if (unknown.length) {
      blocked.push({ id: mutationId, reason: 'unknown-target', targets: unknown.sort() });
      continue;
    }
    const outside = targets.filter(key => !selected.has(key));
    if (outside.length) {
      excluded.push({ id: mutationId, targets: outside.sort() });
      continue;
    }
    const issue = anchorIssue(app, mutation);
    if (issue) {
      blocked.push({ id: mutationId, ...issue });
      continue;
    }

    const targetsBySource = new Map<string, string[]>();
    for (const target of targets) {
      const source = catalog.get(target)?.source as string;
      const sourceTargets = targetsBySource.get(source) ?? [];
      sourceTargets.push(target);
      targetsBySource.set(source, sourceTargets);
    }
    const groups = [...targetsBySource].sort(([left], [right]) => left.localeCompare(right));
    for (const [source, sourceTargets] of groups) {
      mutations.push({
        ...mutation,
        id: groups.length === 1 ? mutationId : `${mutationId}--${scenarioSuffix(source)}`,
        scenario: scenarioPath(compiledRelease.track, source),
        targets: sourceTargets.sort(),
      });
    }
  }

  const ids = mutations.map(mutation => mutation.id);
  if (new Set(ids).size !== ids.length) {
    throw new Error('rebased mutation IDs collide after scenario splitting');
  }
  const covered = [...new Set(mutations.flatMap(mutation => mutation.targets))].sort();
  const coveredSet = new Set(covered);
  const missing = [...selected].filter(key => !coveredSet.has(key)).sort();
  return {
    manifest: {
      schemaVersion: 2,
      status: 'candidate',
      fixtureSha256,
      backend: manifest.backend,
      track: manifest.track as string,
      ...(note ? { note } : {}),
      mutations,
    },
    blocked,
    excluded,
    coverage: {
      selected: [...selected].sort(),
      covered,
      missing,
    },
  };
}
