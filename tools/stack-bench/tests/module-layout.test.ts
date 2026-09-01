import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, extname, join, relative, resolve } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT as ROOT } from '../src/package-root.js';

const SOURCE_AREAS = [
  'actions', 'agents', 'campaigns', 'composition', 'evidence', 'grading',
  'progression', 'references', 'releases', 'runtime', 'stacks',
];
const SKIPPED_DIRECTORIES = new Set([
  'archive', 'dist', 'local-notes', 'node_modules', 'results',
]);
const RELATIVE_MODULE = /(?:from\s+|import\s*\()(['"])(\.\.?\/[^'"\r\n]+?\.js)\1/g;

function filesBelow(directory: string, output: string[] = []): string[] {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory() && SKIPPED_DIRECTORIES.has(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) filesBelow(path, output);
    else if (entry.isFile()) output.push(path);
  }
  return output;
}

function modulesBelow(directory: string): string[] {
  return filesBelow(directory)
    .filter(path => !relative(ROOT, path).split(/[\\/]/).includes('tests'))
    .filter(path => extname(path) === '.ts' && !path.endsWith('.d.ts'));
}

function relativeModuleTargets(file: string): string[] {
  const runtimeSource = readFileSync(file, 'utf8')
    .replace(/(?:import|export)\s+type\b[\s\S]*?\bfrom\s+['"][^'"]+['"];?/g, '');
  return [...runtimeSource.matchAll(RELATIVE_MODULE)]
    .map(match => resolve(dirname(file), `${match[2]?.slice(0, -3)}.ts`));
}

test('the project root contains no implementation modules', () => {
  const rootModules = readdirSync(ROOT, { withFileTypes: true })
    .filter(entry => entry.isFile() && entry.name.endsWith('.ts'))
    .map(entry => entry.name);
  assert.deepEqual(rootModules, []);
  assert.deepEqual(readdirSync(join(ROOT, 'src'), { withFileTypes: true })
    .filter(entry => entry.isDirectory()).map(entry => entry.name).sort(), SOURCE_AREAS);
});

test('tracked production areas contain no tmp-named scratch modules', () => {
  const scratch = modulesBelow(ROOT)
    .map(path => relative(ROOT, path))
    .filter(path => /(?:^|[\\/])tmp-/.test(path));
  assert.deepEqual(scratch, []);
});

test('the authored project contains TypeScript, not JavaScript implementation files', () => {
  const authoredJavaScript = filesBelow(ROOT)
    .filter(path => ['.js', '.mjs'].includes(extname(path)))
    .map(path => relative(ROOT, path));
  assert.deepEqual(authoredJavaScript, []);
});

test('production-relative module references resolve after source reorganization', () => {
  const missing: string[] = [];
  for (const file of modulesBelow(ROOT)) {
    for (const target of relativeModuleTargets(file)) {
      if (!existsSync(target)) missing.push(`${relative(ROOT, file)} -> ${relative(dirname(file), target)}`);
    }
  }
  assert.deepEqual(missing, []);
});

test('production libraries do not import command entrypoints', () => {
  const violations: string[] = [];
  for (const area of ['src', 'grader', 'dashboard']) {
    const directory = join(ROOT, area);
    if (!existsSync(directory)) continue;
    for (const file of modulesBelow(directory)) {
      const source = readFileSync(file, 'utf8');
      if (/from\s+['"][^'"]*commands\/|import\s*\(['"][^'"]*commands\//.test(source)) {
        violations.push(relative(ROOT, file));
      }
    }
  }
  assert.deepEqual(violations, []);
});

test('production modules have no circular imports', () => {
  const files = modulesBelow(ROOT);
  const known = new Set(files);
  const graph = new Map(files.map(file => [file,
    relativeModuleTargets(file).filter(target => known.has(target))]));
  const visited = new Set();
  const active = new Set();
  const stack: string[] = [];
  function visit(file: string): void {
    if (active.has(file)) {
      const cycle = [...stack.slice(stack.indexOf(file)), file]
        .map(path => relative(ROOT, path)).join(' -> ');
      assert.fail(`circular import: ${cycle}`);
    }
    if (visited.has(file)) return;
    visited.add(file);
    active.add(file);
    stack.push(file);
    for (const target of graph.get(file) ?? []) visit(target);
    stack.pop();
    active.delete(file);
  }
  for (const file of files) visit(file);
});

test('package command entrypoints exist', () => {
  const parsed: unknown = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'));
  assert(parsed && typeof parsed === 'object' && 'scripts' in parsed);
  const scripts = parsed.scripts;
  assert(scripts && typeof scripts === 'object');
  for (const [name, command] of Object.entries(scripts)) {
    if (typeof command !== 'string') continue;
    const match = command.match(/^node\s+([^\s]+\.js)(?:\s|$)/);
    if (!match) continue;
    const entrypoint = match[1];
    assert(entrypoint);
    assert.equal(existsSync(join(ROOT, entrypoint)), true,
      `${name} points to missing entrypoint ${entrypoint}`);
  }
});

test('local and appliance compose files pin the same images but isolate resources', () => {
  const local = readFileSync(join(ROOT, 'docker-compose.yaml'), 'utf8');
  const appliance = readFileSync(join(ROOT, 'appliance', 'docker-compose.yaml'), 'utf8');
  const image = (source: string, service: string): string | null => source.match(new RegExp(
    `^  ${service}:\\r?\\n(?:    .*\\r?\\n)*?    image: (\\S+)$`, 'm'))?.[1] ?? null;
  for (const service of ['postgres', 'mongodb']) {
    assert.match(image(local, service) ?? '', /@sha256:[a-f0-9]{64}$/);
    assert.equal(image(local, service), image(appliance, service));
  }
  for (const port of ['6532:5432', '6537:27017']) {
    assert.match(local, new RegExp(`127\\.0\\.0\\.1:${port}`));
    assert.match(appliance, new RegExp(`127\\.0\\.0\\.1:${port}`));
  }
  assert.match(appliance,
    /STACK_BENCH_DASHBOARD_CONTROL_SECRET_FILE: \$\{STACK_BENCH_DASHBOARD_CONTROL_SECRET_FILE:-\/var\/lib\/stack-bench\/secrets\/dashboard_control_secret\}/);
  assert.doesNotMatch(appliance, /STACK_BENCH_DASHBOARD_CONTROL_SECRET:/);
  for (const resource of ['postgres', 'mongodb', 'pgdata', 'mongodata']) {
    assert.match(local, new RegExp(`stack-bench-dev-${resource}`));
    assert.match(appliance, new RegExp(`stack-bench-(?:appliance-)?${resource}`));
    assert.doesNotMatch(appliance, new RegExp(`stack-bench-dev-${resource}`));
  }
});

test('coding containers cannot inspect host-network traffic or gain privileges', () => {
  const source = readFileSync(join(ROOT, 'container', 'run-build.ts'), 'utf8');
  const policy = readFileSync(join(ROOT, 'src', 'runtime', 'coding-container-policy.ts'), 'utf8');
  assert.match(source, /'--cap-drop', 'ALL'/);
  assert.match(source, /'CHOWN', 'DAC_OVERRIDE', 'FOWNER', 'KILL', 'SETGID', 'SETUID'/);
  assert.match(source, /'--security-opt', 'no-new-privileges:true'/);
  assert.match(source, /`127\.0\.0\.1:\$\{p\}:\$\{p\}`/);
  assert.match(source, /'--pids-limit', String\(BUILD_CONTAINER_RESOURCE_LIMITS\.pids\)/);
  assert.match(source, /'--cpus', String\(BUILD_CONTAINER_RESOURCE_LIMITS\.cpuCount\)/);
  assert.match(source, /'--memory', String\(BUILD_CONTAINER_RESOURCE_LIMITS\.memoryBytes\)/);
  assert.match(source,
    /'--memory-swap', String\(BUILD_CONTAINER_RESOURCE_LIMITS\.memorySwapBytes\)/);
  assert.match(source, /resourceLimits: structuredClone\(BUILD_CONTAINER_RESOURCE_LIMITS\)/);
  assert.match(source, /'--read-only'/);
  assert.match(source, /'--user', `\$\{AGENT_UID\}:\$\{AGENT_GID\}`/);
  assert.match(source, /'\/tmp': 'rw,nosuid,nodev,mode=1777'/);
  assert.match(source, /'\/deps': 'rw,nosuid,nodev,mode=0755'/);
  assert.match(source, /\[CONTROL_DIR\]: 'rw,nosuid,nodev,mode=0700'/);
  assert.match(policy, /home: '\/home\/developer'/);
  assert.match(source, /does not have the required isolation/);
  assert.match(source, /STACK_BENCH_APPLIANCE === '1' \? 0o700 : 0o777/);
  assert.match(source, /const CONTROLLER_GID = process\.getgid\?\.\(\) \?\? 0/);
  assert.match(source, /\['chown', \['-R', `\$\{AGENT_UID\}:\$\{CONTROLLER_GID\}`/);
  assert.match(source, /'u\+rwX,g\+rwX,o-rwx'/);
  assert.match(source, /codingContainerWorkspaceHandoffCommands\(CONTROLLER_GID\)/);
  assert.match(source, /'--bare'/);
  assert.match(source, /'--settings', JSON\.stringify\(\{ permissions: \{ allow: \['Bash'\] \} \}\)/);
  assert.doesNotMatch(source, /'\/root':\s*'rw/);
});

test('coding session transcripts are readable by the controller after handoff', () => {
  const source = readFileSync(join(ROOT, 'container', 'run-build.ts'), 'utf8');
  assert.match(source, /codingContainerTranscriptHandoffCommands\(CONTROLLER_GID\)/);
});

test('coding workspaces do not contain harness control files', () => {
  const source = readFileSync(join(ROOT, 'commands', 'agent.ts'), 'utf8');
  assert.doesNotMatch(source, /join\(args\.app, '\.stack-bench-backend'\)/);
  assert.doesNotMatch(source, /join\(args\.app, `\.prompt-/);
  assert.doesNotMatch(source, /join\(args\.app, `\.session-/);
  assert.doesNotMatch(source, /writeSandbox\(args\.app\)/);
  assert.doesNotMatch(source, /--settings|sandboxSettings/);
});
