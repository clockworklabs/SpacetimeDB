#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import {
  existsSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import {
  releasePackages,
  releasePackageName,
  spacetimedbPeerRange,
  spacetimedbVersion,
} from './release-packages.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const failures = [];
const pnpmCommand = process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm';
const canonicalLicense = readFileSync(
  resolve(root, releasePackages[0], 'LICENSE.txt'),
  'utf8'
);

function fail(packageName, message) {
  failures.push(`${packageName}: ${message}`);
}

function readJson(path, packageName) {
  try {
    return JSON.parse(readFileSync(path, 'utf8'));
  } catch (error) {
    fail(
      packageName,
      `invalid JSON in ${relative(root, path)}: ${error.message}`
    );
    return undefined;
  }
}

function isScheduledCallbackAny(node) {
  const callback = node.parent;
  if (!ts.isArrowFunction(callback) || callback.type !== node) return false;
  const property = callback.parent;
  if (!ts.isPropertyAssignment(property)) return false;
  return (
    (ts.isIdentifier(property.name) || ts.isStringLiteral(property.name)) &&
    property.name.text === 'scheduled'
  );
}

function checkExplicitAny(repositoryPath, absolutePath) {
  const source = readFileSync(absolutePath, 'utf8');
  const sourceFile = ts.createSourceFile(
    repositoryPath,
    source,
    ts.ScriptTarget.Latest,
    true
  );
  const visit = node => {
    if (
      node.kind === ts.SyntaxKind.AnyKeyword &&
      !isScheduledCallbackAny(node)
    ) {
      const { line, character } = sourceFile.getLineAndCharacterOfPosition(
        node.getStart(sourceFile)
      );
      fail(
        'repository',
        `${repositoryPath}:${line + 1}:${character + 1} uses explicit any outside the SpacetimeDB scheduled callback boundary`
      );
    }
    ts.forEachChild(node, visit);
  };
  visit(sourceFile);
}

const tracked = spawnSync(
  'git',
  ['ls-files', '--cached', '--others', '--exclude-standard'],
  { cwd: root, encoding: 'utf8', shell: process.platform === 'win32' }
);
if (tracked.status !== 0) {
  fail(
    'repository',
    `could not enumerate files: ${(tracked.stderr || tracked.stdout).trim()}`
  );
} else {
  for (const repositoryPath of tracked.stdout.split(/\r?\n/).filter(Boolean)) {
    const absolutePath = resolve(root, repositoryPath);
    if (!existsSync(absolutePath)) continue;

    const normalizedRepositoryPath = repositoryPath.replaceAll('\\', '/');
    const isSubmodulePath = releasePackages.some(
      packageDir =>
        normalizedRepositoryPath === packageDir ||
        normalizedRepositoryPath.startsWith(`${packageDir}/`)
    );
    if (!isSubmodulePath && normalizedRepositoryPath !== 'pnpm-lock.yaml') {
      continue;
    }
    if (
      /^spacetime-[^/]+-ts\/(?:module|app-module|store-module)(?:\/|$)/.test(
        normalizedRepositoryPath
      ) ||
      /^spacetime-[^/]+-ts\/example\/(?:module|app-module|store-module)(?:\/|$)/.test(
        normalizedRepositoryPath
      )
    ) {
      fail(
        'repository',
        `module directory must be named spacetimedb: ${repositoryPath}`
      );
    }

    if (
      /tools\/(?:run-namespace-cli|use-namespace-cli)/.test(
        normalizedRepositoryPath
      )
    ) {
      fail(
        'repository',
        `obsolete local CLI helper remains: ${repositoryPath}`
      );
    }

    if (
      /\.tsx?$/.test(repositoryPath) &&
      !normalizedRepositoryPath.includes('/codegen/') &&
      !normalizedRepositoryPath.includes('/module_bindings/')
    ) {
      const source = readFileSync(absolutePath, 'utf8');
      if (/\.find\([^\n]*\)\s*(?:===|!==)\s*undefined/.test(source)) {
        fail(
          'repository',
          `${repositoryPath} compares a table lookup with undefined; SpacetimeDB returns null for a missing row`
        );
      }
      checkExplicitAny(repositoryPath, absolutePath);
    }

    if (repositoryPath.endsWith('pnpm-lock.yaml')) {
      const lockfile = readFileSync(absolutePath, 'utf8');
      if (/SpacetimeDBPrivate|spacetimedb@file:/.test(lockfile)) {
        fail(
          'repository',
          `${repositoryPath} resolves SpacetimeDB from a local filesystem path`
        );
      }
      continue;
    }

    if (!repositoryPath.endsWith('package.json')) continue;
    const manifest = readJson(absolutePath, 'repository');
    if (!manifest) continue;
    const workspaceMatch = normalizedRepositoryPath.match(
      /^spacetime-([a-z0-9-]+)-ts\/(example\/spacetimedb|example|spacetimedb)\/package\.json$/
    );
    if (workspaceMatch) {
      const [, slug, workspaceKind] = workspaceMatch;
      const expectedWorkspaceName =
        workspaceKind === 'example'
          ? `spacetime-${slug}-example`
          : workspaceKind === 'example/spacetimedb'
            ? `spacetime-${slug}-example-module`
            : `spacetime-${slug}-module`;
      if (manifest.name !== expectedWorkspaceName) {
        fail(
          'repository',
          `${repositoryPath} name must be ${expectedWorkspaceName}`
        );
      }
    }
    for (const section of [
      'dependencies',
      'devDependencies',
      'optionalDependencies',
    ]) {
      const version = manifest[section]?.spacetimedb;
      if (version && version !== 'workspace:*') {
        fail(
          'repository',
          `${repositoryPath} ${section}.spacetimedb must be workspace:*`
        );
      }
    }
    const peerVersion = manifest.peerDependencies?.spacetimedb;
    if (peerVersion && peerVersion !== spacetimedbPeerRange) {
      fail(
        'repository',
        `${repositoryPath} peerDependencies.spacetimedb must be ${spacetimedbPeerRange}`
      );
    }
  }
}

function exportTargets(exportsField) {
  const targets = [];
  for (const value of Object.values(exportsField ?? {})) {
    if (typeof value === 'string') {
      targets.push(value);
      continue;
    }
    if (value && typeof value === 'object') {
      for (const target of Object.values(value)) {
        if (typeof target === 'string') targets.push(target);
      }
    }
  }
  return [...new Set(targets)];
}

function filesUnder(path) {
  if (!existsSync(path)) return [];
  const out = [];
  for (const entry of readdirSync(path)) {
    const child = resolve(path, entry);
    if (statSync(child).isDirectory()) out.push(...filesUnder(child));
    else out.push(child);
  }
  return out;
}

for (const packageDir of releasePackages) {
  const directory = resolve(root, packageDir);
  const manifestPath = resolve(directory, 'package.json');
  const manifest = readJson(manifestPath, packageDir);
  if (!manifest) continue;

  const packageSlug = packageDir.replace(/^spacetime-/, '');
  const expectedName = releasePackageName(packageDir);
  if (manifest.name !== expectedName)
    fail(packageDir, `name must be ${expectedName}`);
  if (
    !/^(?:0|[1-9]\d*)\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(
      manifest.version ?? ''
    )
  ) {
    fail(packageDir, 'version must be valid semver');
  }
  if (
    typeof manifest.description !== 'string' ||
    manifest.description.length < 20 ||
    manifest.description.length > 180
  ) {
    fail(packageDir, 'description must be 20-180 characters');
  }
  if (manifest.license !== 'BUSL-1.1')
    fail(packageDir, 'license must be BUSL-1.1');
  const packageLicense = readFileSync(
    resolve(directory, 'LICENSE.txt'),
    'utf8'
  );
  if (packageLicense !== canonicalLicense) {
    fail(packageDir, 'LICENSE.txt must match the other release packages');
  }
  if (
    !packageLicense.includes(
      `Licensed Work:        SpacetimeDB ${spacetimedbVersion}`
    )
  ) {
    fail(
      packageDir,
      `LICENSE.txt must cover SpacetimeDB ${spacetimedbVersion}`
    );
  }
  if (manifest.type !== 'module') fail(packageDir, 'type must be module');
  if (
    manifest.main !== './src/index.ts' ||
    manifest.types !== './src/index.ts'
  ) {
    fail(packageDir, 'main and types must point to ./src/index.ts');
  }
  if (manifest.publishConfig?.access !== 'public')
    fail(packageDir, 'publishConfig.access must be public');
  if (manifest.publishConfig?.registry !== 'https://registry.npmjs.org/') {
    fail(
      packageDir,
      'publishConfig.registry must be https://registry.npmjs.org/'
    );
  }
  if (
    manifest.repository?.url !==
    'git+https://github.com/clockworklabs/SpacetimeDB.git'
  ) {
    fail(packageDir, 'repository URL is missing or incorrect');
  }
  if (manifest.repository?.directory !== packageDir)
    fail(packageDir, 'repository.directory must match the package directory');
  if (!manifest.homepage || !manifest.bugs?.url)
    fail(packageDir, 'homepage and bugs metadata are required');
  if (!Array.isArray(manifest.keywords) || manifest.keywords.length < 3)
    fail(packageDir, 'at least three keywords are required');
  if (
    manifest.scripts?.format !==
    'prettier . --write --ignore-path ../.prettierignore'
  ) {
    fail(packageDir, 'format script must match the SpacetimeDB workspace');
  }
  if (
    manifest.scripts?.lint !==
    'eslint . && prettier . --check --ignore-path ../.prettierignore'
  ) {
    fail(packageDir, 'lint script must match the SpacetimeDB workspace');
  }
  if (!manifest.scripts?.typecheck)
    fail(packageDir, 'typecheck script is required');
  if (!manifest.scripts?.test) fail(packageDir, 'a test script is required');

  const rootSource = readFileSync(resolve(directory, 'src/index.ts'), 'utf8');
  const isStandaloneModule =
    /export\s*\{(?=[^}]*\bdefault\b)(?=[^}]*\binit\b)[^}]*\}/s.test(rootSource);
  if (isStandaloneModule) {
    if (manifest.scripts?.build !== 'spacetime build') {
      fail(
        packageDir,
        'standalone module packages must provide a spacetime build script'
      );
    }
    if (
      manifest.scripts?.['spacetime:generate'] !==
      'spacetime generate --lang typescript --out-dir ts-codegen'
    ) {
      fail(
        packageDir,
        'standalone module packages must provide the standard spacetime:generate script'
      );
    }
  }

  for (const requiredFile of ['src', 'README.md', 'LICENSE.txt']) {
    if (!manifest.files?.includes(requiredFile))
      fail(packageDir, `files must include ${requiredFile}`);
    if (!existsSync(resolve(directory, requiredFile)))
      fail(packageDir, `${requiredFile} does not exist`);
  }

  if (!manifest.exports?.['.']) fail(packageDir, 'the root export is required');
  for (const target of exportTargets(manifest.exports)) {
    const normalized = target.replace(/^\.\//, '');
    if (!existsSync(resolve(directory, normalized)))
      fail(packageDir, `export target does not exist: ${target}`);
    const included = manifest.files?.some(
      entry => normalized === entry || normalized.startsWith(`${entry}/`)
    );
    if (!included)
      fail(packageDir, `export target is excluded from the tarball: ${target}`);
  }

  const dependencySections = ['dependencies', 'optionalDependencies'];
  for (const section of dependencySections) {
    for (const [name, version] of Object.entries(manifest[section] ?? {})) {
      if (/^(?:file|link):/.test(version))
        fail(
          packageDir,
          `${section}.${name} must not use a filesystem dependency`
        );
      if (name.startsWith('@spacetimedb/') && version !== 'workspace:^') {
        fail(packageDir, `${section}.${name} must be workspace:^`);
      }
    }
  }

  if (packageSlug !== 'crypto-ts') {
    if (manifest.peerDependencies?.spacetimedb !== spacetimedbPeerRange) {
      fail(
        packageDir,
        `spacetimedb peer dependency must be ${spacetimedbPeerRange}`
      );
    }
    if (Object.hasOwn(manifest.scripts ?? {}, 'publish')) {
      fail(
        packageDir,
        'scripts.publish is an npm lifecycle hook; use an explicit name such as publish:module'
      );
    }
    if (manifest.devDependencies?.spacetimedb !== 'workspace:*') {
      fail(packageDir, 'spacetimedb devDependency must be workspace:*');
    }
  }

  const readme = readFileSync(resolve(directory, 'README.md'), 'utf8');
  const firstLine = readme.split(/\r?\n/, 1)[0];
  if (firstLine !== `# ${expectedName}`)
    fail(packageDir, `README must start with # ${expectedName}`);
  for (const heading of ['Install', 'Testing', 'License']) {
    if (!new RegExp(`^## ${heading}\\s*$`, 'm').test(readme))
      fail(packageDir, `README is missing the ${heading} section`);
  }
  if (readme.split(/\r?\n/).length > 400)
    fail(
      packageDir,
      'README exceeds 400 lines; move internal design notes elsewhere'
    );
  for (const smell of [
    '## Backlog',
    '## Roadmap',
    'What this package actually contains',
    'What STDB needs to ship for production',
  ]) {
    if (readme.includes(smell))
      fail(packageDir, `README contains internal or draft wording: ${smell}`);
  }

  const submoduleTarget = manifest.exports?.['./submodule']?.default;
  if (submoduleTarget) {
    const source = readFileSync(
      resolve(directory, submoduleTarget.replace(/^\.\//, '')),
      'utf8'
    );
    if (/export\s*\{[^}]*\binit\b[^}]*\}/s.test(source))
      fail(packageDir, './submodule must not export init');
  }

  for (const sourcePath of [
    ...filesUnder(resolve(directory, 'src')),
    ...filesUnder(resolve(directory, 'spacetimedb', 'src')),
  ]) {
    if (!sourcePath.endsWith('.ts')) continue;
    const source = readFileSync(sourcePath, 'utf8');
    const sourceName = relative(root, sourcePath).split(sep).join('/');
    if (/@ts-(?:ignore|nocheck)/.test(source))
      fail(packageDir, `${sourceName} disables TypeScript checking`);
    if (/from\s+['"]node:|\brequire\s*\(|\bprocess\./.test(source))
      fail(packageDir, `${sourceName} imports a Node-only API`);
  }

  const packDirectory = mkdtempSync(join(tmpdir(), 'stdb-submodule-pack-'));
  const packed = spawnSync(
    pnpmCommand,
    ['pack', '--json', '--pack-destination', packDirectory],
    { cwd: directory, encoding: 'utf8', shell: process.platform === 'win32' }
  );
  if (packed.status !== 0) {
    const detail =
      packed.error?.message ||
      packed.stderr ||
      packed.stdout ||
      `exit ${packed.status}`;
    fail(packageDir, `pnpm pack failed: ${detail.trim()}`);
    rmSync(packDirectory, { force: true, recursive: true });
    continue;
  }
  let packResult;
  try {
    packResult = JSON.parse(packed.stdout);
  } catch (error) {
    fail(packageDir, `could not parse pnpm pack output: ${error.message}`);
    rmSync(packDirectory, { force: true, recursive: true });
    continue;
  }
  const packedFiles = (packResult.files ?? []).map(entry =>
    entry.path.replaceAll('\\', '/')
  );
  for (const required of ['package.json', 'README.md', 'LICENSE.txt']) {
    if (!packedFiles.includes(required))
      fail(packageDir, `tarball is missing ${required}`);
  }
  for (const path of packedFiles) {
    if (
      /^(?:example|scripts|node_modules|ts-codegen|dist|target)\//.test(path) ||
      path === 'pnpm-lock.yaml'
    ) {
      fail(packageDir, `tarball contains development-only file: ${path}`);
    }
  }
  rmSync(packDirectory, { force: true, recursive: true });
}

if (failures.length > 0) {
  console.error(`Release check failed with ${failures.length} issue(s):`);
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(
  `Release-preparation check passed for ${releasePackages.length} packages.`
);
