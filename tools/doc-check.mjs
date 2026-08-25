#!/usr/bin/env node

import { createRequire } from 'node:module';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { releasePackages } from './release-packages.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const requireFromPackage = createRequire(
  resolve(root, 'spacetime-agents-ts', 'package.json')
);
const ts = requireFromPackage('typescript');
const failures = [];

const documentationFiles = [
  ...releasePackages.flatMap(packageDir => {
    const files = [`${packageDir}/README.md`];
    const exampleReadme = `${packageDir}/example/README.md`;
    if (existsSync(resolve(root, exampleReadme))) files.push(exampleReadme);
    return files;
  }),
];

function fail(file, line, message) {
  failures.push(`${file}:${line}: ${message}`);
}

function lineNumberAt(text, offset) {
  return text.slice(0, offset).split('\n').length;
}

function validateLinks(file, text) {
  const linkPattern = /!?\[[^\]]*\]\(([^)]+)\)/g;
  for (const match of text.matchAll(linkPattern)) {
    let target = match[1].trim();
    if (target.startsWith('<') && target.endsWith('>'))
      target = target.slice(1, -1);
    target = target.split(/\s+["']/)[0];
    if (/^(?:https?:|mailto:|#)/i.test(target)) continue;
    const path = decodeURIComponent(target.split('#')[0]);
    if (!path) continue;
    if (!existsSync(resolve(root, dirname(file), path))) {
      fail(
        file,
        lineNumberAt(text, match.index),
        `broken relative link: ${target}`
      );
    }
  }
}

function validateFences(file, text) {
  const lines = text.split(/\r?\n/);
  let fence;
  for (let index = 0; index < lines.length; index += 1) {
    const line = lines[index];
    const opening = line.match(/^```\s*([A-Za-z0-9_-]*)\s*$/);
    if (!fence && opening) {
      fence = {
        language: opening[1].toLowerCase(),
        start: index + 1,
        lines: [],
      };
      continue;
    }
    if (fence && /^```\s*$/.test(line)) {
      if (['ts', 'typescript', 'js', 'javascript'].includes(fence.language)) {
        const source = fence.lines.join('\n');
        const result = ts.transpileModule(source, {
          compilerOptions: {
            target: ts.ScriptTarget.ES2022,
            module: ts.ModuleKind.ESNext,
          },
          fileName: `${file}.${fence.language.startsWith('j') ? 'js' : 'ts'}`,
          reportDiagnostics: true,
        });
        for (const diagnostic of result.diagnostics ?? []) {
          if (diagnostic.category !== ts.DiagnosticCategory.Error) continue;
          const position =
            diagnostic.file && diagnostic.start != null
              ? diagnostic.file.getLineAndCharacterOfPosition(diagnostic.start)
              : { line: 0 };
          fail(
            file,
            fence.start + 1 + position.line,
            `invalid ${fence.language} example: ${ts.flattenDiagnosticMessageText(diagnostic.messageText, ' ')}`
          );
        }
      }
      fence = undefined;
      continue;
    }
    if (fence) fence.lines.push(line);
  }
  if (fence) fail(file, fence.start, 'unclosed code fence');
}

for (const file of documentationFiles) {
  const path = resolve(root, file);
  if (!existsSync(path)) {
    fail(file, 1, 'documentation file is missing');
    continue;
  }
  const text = readFileSync(path, 'utf8');
  validateLinks(file, text);
  validateFences(file, text);
  if (/\b(?:TODO|FIXME|WIP)\b/i.test(text)) {
    fail(file, 1, 'contains draft-work marker (TODO/FIXME/WIP)');
  }
  if (/\bnamespace[- ]branch\b/i.test(text)) {
    fail(file, 1, 'contains obsolete namespace-branch wording');
  }
  if (/SpacetimeDBPrivate/.test(text)) {
    fail(file, 1, 'leaks a contributor-local repository name');
  }
}

for (const packageDir of releasePackages) {
  const file = `${packageDir}/README.md`;
  const text = readFileSync(resolve(root, file), 'utf8');
  const manifest = JSON.parse(
    readFileSync(resolve(root, packageDir, 'package.json'), 'utf8')
  );
  const firstLine = text.split(/\r?\n/, 1)[0];
  if (firstLine !== `# ${manifest.name}`)
    fail(file, 1, `must start with # ${manifest.name}`);
  const requiredHeadings = ['Install', 'Usage', 'API', 'Testing', 'License'];
  let previousHeadingOffset = -1;
  for (const heading of requiredHeadings) {
    const match = new RegExp(`^## ${heading}(?:\\s|$)`, 'mi').exec(text);
    if (!match) {
      fail(file, 1, `missing release README section: ${heading}`);
      continue;
    }
    if (match.index < previousHeadingOffset) {
      fail(
        file,
        lineNumberAt(text, match.index),
        `release README section is out of order: ${heading}`
      );
    }
    previousHeadingOffset = match.index;
  }
  if (!/^### Integrate into an application\s*$/m.test(text)) {
    fail(
      file,
      1,
      'missing consumer onboarding section: Integrate into an application'
    );
  }
  if (
    !text.includes('spacetimedb@^2.8.3') &&
    packageDir !== 'spacetime-crypto-ts'
  ) {
    fail(
      file,
      1,
      'install command must pin the compatible SpacetimeDB 2.8 peer range'
    );
  }
  if (!text.includes('https://spacetimedb.com/docs/')) {
    fail(file, 1, 'missing link to the official getting-started guide');
  }
}

for (const packageDir of releasePackages) {
  const file = `${packageDir}/example/README.md`;
  const path = resolve(root, file);
  if (!existsSync(path)) continue;
  const text = readFileSync(path, 'utf8');
  for (const heading of [
    'Prerequisites',
    'Quick start',
    'Use in your project',
  ]) {
    if (!new RegExp(`^## ${heading}\\s*$`, 'm').test(text)) {
      fail(file, 1, `missing example onboarding section: ${heading}`);
    }
  }
  if (!text.includes('spacetime version use 2.8.3')) {
    fail(file, 1, 'quick start must select SpacetimeDB CLI 2.8.3');
  }
  if (!text.includes('spacetime start')) {
    fail(file, 1, 'quick start must explain how to start the local server');
  }
}

if (failures.length > 0) {
  console.error(`Documentation check failed with ${failures.length} issue(s):`);
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(
  `Documentation check passed for ${documentationFiles.length} files.`
);
