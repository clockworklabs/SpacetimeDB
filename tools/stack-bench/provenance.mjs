import { createHash } from 'node:crypto';
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';

export const sha256 = value => createHash('sha256').update(value).digest('hex');

// Hash paths and bytes with explicit framing. Hashing concatenated contents is
// ambiguous (`ab`+`c` equals `a`+`bc`) and loses which file supplied a rubric.
export function hashFiles(paths, { base = process.cwd() } = {}) {
  const entries = [...new Set(paths.map(path => resolve(path)))]
    .map(path => ({ path, name: relative(resolve(base), path).replaceAll('\\', '/') }))
    .sort((a, b) => a.name.localeCompare(b.name));
  const hash = createHash('sha256');
  for (const entry of entries) {
    const bytes = readFileSync(entry.path);
    hash.update(`${entry.name.length}:${entry.name}:${bytes.length}:`);
    hash.update(bytes);
  }
  return { sha256: hash.digest('hex'), files: entries.map(entry => entry.name) };
}

export function hashDirectory(root, { exclude = () => false } = {}) {
  const files = [];
  const walk = directory => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      const name = relative(root, path).replaceAll('\\', '/');
      if (exclude(name, entry)) continue;
      if (entry.isDirectory()) walk(path);
      else if (entry.isFile()) files.push(path);
    }
  };
  walk(resolve(root));
  return hashFiles(files, { base: root });
}

// The rubric is the point-bearing definition, distinct from scenario mechanics.
// A timing or selector edit changes scenariosSha256; a criterion/points edit
// also changes rubricSha256 and therefore invalidates score comparison.
export function hashRubric(paths, { base = process.cwd() } = {}) {
  const rows = [];
  for (const path of [...new Set(paths.map(item => resolve(item)))].sort()) {
    const spec = JSON.parse(readFileSync(path, 'utf8'));
    for (const feature of spec.features ?? []) {
      for (const criterion of feature.criteria ?? []) {
        rows.push({
          file: relative(resolve(base), path).replaceAll('\\', '/'),
          feature: String(feature.id ?? feature.name ?? ''),
          criterion: String(criterion.id),
          points: Number(criterion.points ?? 0),
        });
      }
    }
  }
  rows.sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  return { sha256: sha256(JSON.stringify(rows)), criteria: rows.length,
    points: rows.reduce((total, row) => total + row.points, 0) };
}

export function sessionProvenance({ prompt, skillsText = '', contractText = '',
  bugReportText = null, scenarioPaths, trackDir, trackManifestPath }) {
  const scenarios = hashFiles(scenarioPaths, { base: trackDir });
  return {
    promptSha256: sha256(prompt),
    skillsSha256: sha256(skillsText),
    contractSha256: sha256(contractText),
    bugReportSha256: bugReportText == null ? null : sha256(bugReportText),
    trackManifestSha256: sha256(readFileSync(trackManifestPath)),
    scenariosSha256: scenarios.sha256,
    scenarioFiles: scenarios.files,
    rubric: hashRubric(scenarioPaths, { base: trackDir }),
  };
}
