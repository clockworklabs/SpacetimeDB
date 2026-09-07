import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import { pathToFileURL } from 'node:url';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';

function currentIdentity(root: string): unknown {
  const artifactsUrl = pathToFileURL(join(root, 'dist', 'src', 'evidence', 'artifacts.js')).href;
  const result = spawnSync(process.execPath, ['--input-type=module', '--eval',
    `import { currentEngineIdentity } from ${JSON.stringify(artifactsUrl)}; console.log(JSON.stringify(currentEngineIdentity()));`],
  { cwd: root, encoding: 'utf8' });
  assert.equal(result.status, 0, result.stderr);
  return JSON.parse(result.stdout ?? '');
}

function writableEngineRoot(): { temp: string; root: string } {
  const temp = mkdtempSync(join(tmpdir(), 'stack-bench-engine-'));
  const root = join(temp, 'stack-bench');
  const excluded = new Set(['archive', 'local-notes', 'media', 'node_modules',
    'qualification-evidence', 'reference-apps', 'results', 'tests', 'tracks', 'transcripts']);
  cpSync(STACK_BENCH_ROOT, root, { recursive: true, filter: source => {
    if (source === STACK_BENCH_ROOT) return true;
    return !excluded.has(relative(STACK_BENCH_ROOT, source).split(/[\\/]/)[0] ?? '');
  } });
  mkdirSync(join(root, 'node_modules'), { recursive: true });
  cpSync(join(STACK_BENCH_ROOT, 'node_modules', 'zod'), join(root, 'node_modules', 'zod'),
    { recursive: true });
  return { temp, root };
}

test('engine identity excludes generated and local-only files', () => {
  const copy = writableEngineRoot();
  const before = currentIdentity(copy.root);
  try {
    const root = copy.root;
    const runtime = mkdtempSync(join(root, '.engine-runtime-'));
    writeFileSync(join(runtime, 'generated.mjs'), 'generated runtime state\n');
    writeFileSync(join(runtime, 'session.json'), '{"turns":999}\n');
    const nested = join(root, 'grader', '.generated');
    mkdirSync(nested, { recursive: true });
    writeFileSync(join(nested, 'candidate.mjs'), 'generated candidate\n');
    writeFileSync(join(root, 'grader', '.generated-report.json'), '{"generated":true}\n');

    for (const directory of ['archive', 'local-notes', 'media', 'qualification-evidence',
      'transcripts']) {
      const path = join(root, directory);
      mkdirSync(path, { recursive: true });
      writeFileSync(join(path, 'generated.json'), '{"generated":true}\n');
    }
    writeFileSync(join(root, 'dependency-manifest.json'), '{"schemaVersion":1}\n');

    const path = join(root, 'grader', 'node_modules', 'generated-package');
    mkdirSync(path, { recursive: true });
    writeFileSync(join(path, 'index.js'), 'installed dependency\n');
    writeFileSync(join(path, 'package.json'), '{"name":"generated-package"}\n');

    assert.deepEqual(currentIdentity(root), before);
  } finally {
    rmSync(copy.temp, { recursive: true, force: true });
  }
});
