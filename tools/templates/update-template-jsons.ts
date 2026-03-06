/**
 * Updates .template.json in each template folder with builtWith derived from
 * package.json, Cargo.toml, and .csproj manifests. Run from SpacetimeDB repo root.
 *
 * Writes to templates/<slug>/.template.json. Commit those changes to keep
 * template metadata in sync with spacetimedb.com.
 *
 * Usage: pnpm run update-jsons (from tools/templates/)
 */

import { readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '../..');
const TEMPLATES_DIR = path.join(REPO_ROOT, 'templates');

const PACKAGE_REFERENCE_RE = /PackageReference\s+Include="([^"]+)"/g;

/** Normalize npm package name: @scope/pkg → scope, else use as-is */
function normalizeNpmPackageName(name: string): string {
  if (name.startsWith('@')) {
    const slash = name.indexOf('/');
    return slash > 0 ? name.slice(1, slash) : name.slice(1);
  }
  return name;
}

/** Skip @types/* packages - typings, not frameworks */
function shouldSkipPackage(normalized: string): boolean {
  return normalized === 'types';
}

function parsePackageJson(content: string): string[] {
  const result: string[] = [];
  let pkg: {
    dependencies?: Record<string, unknown>;
    devDependencies?: Record<string, unknown>;
  };
  try {
    pkg = JSON.parse(content);
  } catch {
    return result;
  }
  for (const deps of [pkg.dependencies, pkg.devDependencies]) {
    if (deps && typeof deps === 'object') {
      for (const name of Object.keys(deps)) {
        const normalized = normalizeNpmPackageName(name);
        if (!shouldSkipPackage(normalized)) result.push(normalized);
      }
    }
  }
  return result;
}

/** Parse [dependencies] section from Cargo.toml. Keys only, no external deps. */
function parseCargoToml(content: string): string[] {
  const result: string[] = [];
  const lines = content.split(/\r?\n/);
  let inDependencies = false;
  for (const line of lines) {
    const trimmed = line.trim();
    if (trimmed.startsWith('[')) {
      inDependencies = trimmed === '[dependencies]';
      continue;
    }
    if (inDependencies && trimmed && !trimmed.startsWith('#')) {
      const eq = trimmed.indexOf('=');
      if (eq > 0) {
        const key = trimmed.slice(0, eq).trim();
        if (key) result.push(key);
      }
    }
  }
  return result;
}

function parseCsproj(content: string): string[] {
  const result: string[] = [];
  for (const match of content.matchAll(PACKAGE_REFERENCE_RE)) {
    result.push(match[1]);
  }
  return result;
}

async function findManifests(
  dir: string
): Promise<{ packageJson: string[]; cargoToml: string[]; csproj: string[] }> {
  const packageJson: string[] = [];
  const cargoToml: string[] = [];
  const csproj: string[] = [];

  async function walk(currentDir: string): Promise<void> {
    let entries: import('node:fs').Dirent[];
    try {
      entries = await readdir(currentDir, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      const fullPath = path.join(currentDir, entry.name);
      if (entry.isDirectory()) {
        if (!entry.name.startsWith('.') && entry.name !== 'node_modules') {
          await walk(fullPath);
        }
      } else if (entry.isFile()) {
        if (entry.name === 'package.json') {
          packageJson.push(fullPath);
        } else if (entry.name === 'Cargo.toml') {
          cargoToml.push(fullPath);
        } else if (entry.name.endsWith('.csproj')) {
          csproj.push(fullPath);
        }
      }
    }
  }

  await walk(dir);
  return { packageJson, cargoToml, csproj };
}

/** Sort paths so root manifests come before subdirs (e.g. root package.json before spacetimedb/package.json) */
function sortRootFirst(paths: string[]): string[] {
  return [...paths].sort(
    (a, b) => a.split(path.sep).length - b.split(path.sep).length
  );
}

async function collectDepsFromManifests(
  templateDir: string,
  slug: string
): Promise<string[]> {
  const seen = new Set<string>();
  const { packageJson, cargoToml, csproj } = await findManifests(templateDir);
  const isNodeTemplate = slug.includes('nodejs');

  for (const filePath of sortRootFirst(packageJson)) {
    try {
      const content = await readFile(filePath, 'utf-8');
      const pkg = JSON.parse(content) as {
        dependencies?: Record<string, unknown>;
        devDependencies?: Record<string, unknown>;
      };
      if (
        isNodeTemplate &&
        ((pkg.dependencies && '@types/node' in pkg.dependencies) ||
          (pkg.devDependencies && '@types/node' in pkg.devDependencies))
      ) {
        seen.add('nodejs');
      }
      for (const dep of parsePackageJson(content)) {
        seen.add(dep);
      }
    } catch {
      // skip
    }
  }

  for (const filePath of sortRootFirst(cargoToml)) {
    try {
      const content = await readFile(filePath, 'utf-8');
      for (const dep of parseCargoToml(content)) {
        seen.add(dep);
      }
    } catch {
      // skip
    }
  }

  for (const filePath of sortRootFirst(csproj)) {
    try {
      const content = await readFile(filePath, 'utf-8');
      for (const dep of parseCsproj(content)) {
        seen.add(dep);
      }
    } catch {
      // skip
    }
  }

  const deps = [...seen];
  const stdb: string[] = [];
  const rest: string[] = [];
  for (const d of deps) {
    if (d.startsWith('spacetimedb') || d.startsWith('SpacetimeDB')) {
      stdb.push(d);
    } else {
      rest.push(d);
    }
  }
  return [...rest, ...stdb];
}

export async function updateTemplateJsons(): Promise<void> {
  let entries: import('node:fs').Dirent[];
  try {
    entries = await readdir(TEMPLATES_DIR, { withFileTypes: true });
  } catch (err) {
    console.warn(`Could not read templates dir: ${TEMPLATES_DIR}`, err);
    return;
  }

  const dirs = entries
    .filter(
      (e): e is import('node:fs').Dirent =>
        e.isDirectory() && !e.name.startsWith('.')
    )
    .map(e => e.name);

  let updated = 0;
  for (const slug of dirs) {
    const templateDir = path.join(TEMPLATES_DIR, slug);
    const jsonPath = path.join(templateDir, '.template.json');
    let jsonRaw: string;
    try {
      jsonRaw = await readFile(jsonPath, 'utf-8');
    } catch {
      continue;
    }

    let meta: Record<string, unknown>;
    try {
      meta = JSON.parse(jsonRaw);
    } catch {
      console.warn(`Skipping ${slug}: invalid JSON`);
      continue;
    }

    const builtWith = await collectDepsFromManifests(templateDir, slug);

    const { image: _image, ...rest } = meta;
    const updatedMeta = { ...rest, builtWith };

    const updatedJson = JSON.stringify(updatedMeta, null, 2) + '\n';
    if (updatedJson !== jsonRaw) {
      await writeFile(jsonPath, updatedJson);
      console.log(`Updated ${slug}/.template.json`);
      updated++;
    }
  }

  console.log(`Updated ${updated} template JSON(s)`);
}

const isMain =
  import.meta.url === `file://${process.argv[1]}` ||
  fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (isMain) {
  updateTemplateJsons().catch(err => {
    console.error(err);
    process.exit(1);
  });
}
