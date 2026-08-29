import { existsSync, readFileSync } from 'node:fs';
import { basename } from 'node:path';

import { mutationFileEdits, mutationTargetKeys, resolveMutationFile,
  validateMutationDefinitions } from './mutation-analysis.js';

function scenarioPath(track, source) {
  return `tracks/${track}/${source}`;
}

function scenarioSuffix(source) {
  return basename(source, '.json')
    .replace(/^progression-/, '')
    .replace(/-\d+\.\d+\.\d+$/, '');
}

function anchorIssue(app, mutation) {
  for (const edit of mutationFileEdits(mutation)) {
    let file;
    try {
      file = resolveMutationFile(app, edit.file);
    } catch (error) {
      return { reason: 'unsafe-file', detail: error.message };
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

function selectedKeys(release, keys) {
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
  return { catalog, selected: new Set(keys) };
}

export function rebaseMutationManifest(manifest,
  { release, selectedCheckKeys, app, fixtureSha256, note } = {}) {
  if (manifest?.schemaVersion !== 2 || manifest.level !== undefined) {
    throw new Error('mutation rebase accepts only level-independent schema 2 manifests');
  }
  if (!Array.isArray(manifest.mutations) || manifest.mutations.length === 0) {
    throw new Error('mutation rebase requires a non-empty mutation manifest');
  }
  if (typeof app !== 'string' || !app || !/^[a-f0-9]{64}$/.test(fixtureSha256 ?? '')) {
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

  const { catalog, selected } = selectedKeys(release, selectedCheckKeys);
  const blocked = [];
  const excluded = [];
  const mutations = [];
  for (const mutation of manifest.mutations) {
    const targets = mutationTargetKeys(mutation);
    const unknown = targets.filter(key => !catalog.has(key));
    if (unknown.length) {
      blocked.push({ id: mutation.id, reason: 'unknown-target', targets: unknown.sort() });
      continue;
    }
    const outside = targets.filter(key => !selected.has(key));
    if (outside.length) {
      excluded.push({ id: mutation.id, targets: outside.sort() });
      continue;
    }
    const issue = anchorIssue(app, mutation);
    if (issue) {
      blocked.push({ id: mutation.id, ...issue });
      continue;
    }

    const targetsBySource = new Map();
    for (const target of targets) {
      const source = catalog.get(target).source;
      if (!targetsBySource.has(source)) targetsBySource.set(source, []);
      targetsBySource.get(source).push(target);
    }
    const groups = [...targetsBySource].sort(([left], [right]) => left.localeCompare(right));
    for (const [source, sourceTargets] of groups) {
      mutations.push({
        ...mutation,
        id: groups.length === 1 ? mutation.id : `${mutation.id}--${scenarioSuffix(source)}`,
        scenario: scenarioPath(release.track, source),
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
      track: manifest.track,
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
