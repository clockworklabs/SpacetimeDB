#!/usr/bin/env node

import fs from 'node:fs/promises';
import path from 'node:path';

const cwd = process.cwd();
const repoRoot = path.basename(cwd) === 'docs' ? path.dirname(cwd) : cwd;
const docsDir = path.basename(cwd) === 'docs' ? cwd : path.join(repoRoot, 'docs');
const args = new Set(process.argv.slice(2));
const write = args.has('--write');
const verbose = args.has('--verbose');

const isDocFile = (p) => p.endsWith('.md') || p.endsWith('.mdx');

async function listFilesRecursive(dir) {
  const out = [];
  async function walk(current) {
    const entries = await fs.readdir(current, { withFileTypes: true });
    for (const entry of entries) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        await walk(full);
      } else if (entry.isFile() && isDocFile(full)) {
        out.push(full);
      }
    }
  }
  await walk(dir);
  return out;
}

function parseFrontMatter(content) {
  if (!content.startsWith('---\n')) return {};
  const end = content.indexOf('\n---\n', 4);
  if (end === -1) return {};
  const body = content.slice(4, end);
  const out = {};
  for (const line of body.split('\n')) {
    const m = line.match(/^([A-Za-z_][A-Za-z0-9_-]*):\s*(.+)\s*$/);
    if (!m) continue;
    const key = m[1];
    let value = m[2].trim();
    if (
      (value.startsWith('"') && value.endsWith('"')) ||
      (value.startsWith("'") && value.endsWith("'"))
    ) {
      value = value.slice(1, -1);
    }
    out[key] = value;
  }
  return out;
}

function stripOrderingPrefix(segment) {
  return segment.replace(/^\d{3,6}-/, '');
}

function normalizeRoute(route) {
  if (!route) return '/';
  let out = route.trim();
  if (!out.startsWith('/')) out = `/${out}`;
  out = out.replace(/\/{2,}/g, '/');
  if (out.length > 1) out = out.replace(/\/+$/, '');
  return out;
}

function defaultRouteFromFile(namespaceRoot, fullPath) {
  const rel = path.posix.normalize(path.relative(namespaceRoot, fullPath)).replace(/\\/g, '/');
  const parsed = path.posix.parse(rel);
  const parts = parsed.dir === '.' ? [] : parsed.dir.split('/').map(stripOrderingPrefix);
  const base = stripOrderingPrefix(parsed.name);
  if (base !== 'index') parts.push(base);
  return normalizeRoute(parts.join('/'));
}

function splitPathSuffix(target) {
  const m = target.match(/^([^?#]*)([?#].*)?$/);
  return { pathPart: m?.[1] ?? target, suffix: m?.[2] ?? '' };
}

function buildLookupCandidates(rawPath, knownVersionNames) {
  const pathPart = normalizeRoute(rawPath);
  const candidates = new Set([pathPart]);

  if (pathPart.startsWith('/docs/')) {
    candidates.add(normalizeRoute(pathPart.slice('/docs'.length)));
  } else if (pathPart === '/docs') {
    candidates.add('/');
  }

  for (const versionName of knownVersionNames) {
    const prefix = `/${versionName}/`;
    if (pathPart.startsWith(prefix)) {
      candidates.add(normalizeRoute(pathPart.slice(versionName.length + 1)));
    }
    const docsPrefix = `/docs/${versionName}/`;
    if (pathPart.startsWith(docsPrefix)) {
      candidates.add(normalizeRoute(pathPart.slice(versionName.length + '/docs'.length + 1)));
    }
  }

  return [...candidates];
}

function toRelativeLink(fromFile, toFile, suffix) {
  const fromDir = path.posix.dirname(fromFile.replace(/\\/g, '/'));
  const toPosix = toFile.replace(/\\/g, '/');
  let rel = path.posix.relative(fromDir, toPosix);
  if (!rel) {
    return suffix || '#';
  }
  if (!rel.startsWith('.')) rel = `./${rel}`;
  return `${rel}${suffix}`;
}

async function detectNamespaces() {
  const namespaces = [];
  const currentDocs = path.join(docsDir, 'docs');
  try {
    const stat = await fs.stat(currentDocs);
    if (stat.isDirectory()) namespaces.push(currentDocs);
  } catch {}

  const versionedRoot = path.join(docsDir, 'versioned_docs');
  try {
    const entries = await fs.readdir(versionedRoot, { withFileTypes: true });
    for (const e of entries) {
      if (e.isDirectory() && e.name.startsWith('version-')) {
        namespaces.push(path.join(versionedRoot, e.name));
      }
    }
  } catch {}

  return namespaces;
}

function markdownLinkRewriter(content, rewriter) {
  const pattern = /\[[^\]]*?\]\((\/[^)\s]*?)\)/g;
  return content.replace(pattern, (full, target, offset) => {
    if (offset > 0 && content[offset - 1] === '!') return full;
    const replaced = rewriter(target);
    if (!replaced || replaced === target) return full;
    return full.replace(`(${target})`, `(${replaced})`);
  });
}

function mdxLinkRewriter(content, rewriter) {
  const pattern = /<Link\b([^>]*?\bto=)(["'])(\/[^"'{}]+)\2([^>]*)>/g;
  return content.replace(pattern, (full, before, quote, target, after) => {
    const replaced = rewriter(target);
    if (!replaced || replaced === target) return full;
    return `<Link${before}${quote}${replaced}${quote}${after}>`;
  });
}

async function main() {
  const namespaces = await detectNamespaces();
  if (namespaces.length === 0) {
    console.error('No docs namespaces found.');
    process.exit(1);
  }

  const versionsPath = path.join(docsDir, 'versions.json');
  let knownVersionNames = ['v1', 'prerelease'];
  try {
    const raw = await fs.readFile(versionsPath, 'utf8');
    const versions = JSON.parse(raw);
    if (Array.isArray(versions)) {
      knownVersionNames = [...new Set([...knownVersionNames, ...versions])];
    }
  } catch {}
  knownVersionNames.push('2.0.0-rc1');
  knownVersionNames = [...new Set(knownVersionNames)];

  let changedFiles = 0;
  let rewrittenLinks = 0;
  const unresolved = [];

  for (const namespace of namespaces) {
    const files = await listFilesRecursive(namespace);
    const routeToFile = new Map();
    const collisions = new Map();

    for (const file of files) {
      const content = await fs.readFile(file, 'utf8');
      const fm = parseFrontMatter(content);

      const candidates = new Set([defaultRouteFromFile(namespace, file)]);
      if (typeof fm.slug === 'string' && fm.slug) {
        candidates.add(normalizeRoute(fm.slug));
      }

      for (const route of candidates) {
        if (routeToFile.has(route) && routeToFile.get(route) !== file) {
          collisions.set(route, true);
          continue;
        }
        routeToFile.set(route, file);
      }
    }

    for (const file of files) {
      const relFile = path.relative(repoRoot, file).replace(/\\/g, '/');
      const original = await fs.readFile(file, 'utf8');
      let linkChanges = 0;

      const rewriteTarget = (target) => {
        if (!target.startsWith('/') || target.startsWith('//')) return target;
        const { pathPart, suffix } = splitPathSuffix(target);
        const lookupPaths = buildLookupCandidates(pathPart, knownVersionNames);

        let resolvedFile;
        for (const candidate of lookupPaths) {
          if (collisions.has(candidate)) continue;
          const hit = routeToFile.get(candidate);
          if (hit) {
            resolvedFile = hit;
            break;
          }
        }
        if (!resolvedFile) {
          unresolved.push({ file: relFile, target });
          return target;
        }

        const relative = toRelativeLink(file, resolvedFile, suffix);
        if (relative !== target) {
          linkChanges += 1;
        }
        return relative;
      };

      let updated = markdownLinkRewriter(original, rewriteTarget);
      updated = mdxLinkRewriter(updated, rewriteTarget);

      if (updated !== original) {
        changedFiles += 1;
        rewrittenLinks += linkChanges;
        if (write) {
          await fs.writeFile(file, updated, 'utf8');
        }
        if (verbose) {
          console.log(`${write ? 'rewrote' : 'would rewrite'} ${relFile} (${linkChanges} links)`);
        }
      }
    }
  }

  console.log(
    `${write ? 'Rewrote' : 'Would rewrite'} ${rewrittenLinks} link(s) in ${changedFiles} file(s).`,
  );

  if (unresolved.length > 0) {
    const unique = new Map();
    for (const item of unresolved) {
      const key = `${item.file} -> ${item.target}`;
      unique.set(key, item);
    }
    const list = [...unique.values()];
    console.log(`Skipped ${list.length} unresolved absolute link(s).`);
    for (const item of list.slice(0, 50)) {
      console.log(`  ${item.file}: ${item.target}`);
    }
    if (list.length > 50) {
      console.log(`  ...and ${list.length - 50} more`);
    }
  }

  if (!write) {
    console.log('Dry run only. Re-run with --write to apply changes.');
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
