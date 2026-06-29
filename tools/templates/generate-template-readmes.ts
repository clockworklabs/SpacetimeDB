/**
 * Reads SpacetimeDB quickstart MDX docs, converts them to plain Markdown,
 * and writes README.md into each template folder. These READMEs are consumed
 * by spacetimedb.com's process-templates to generate the templates page.
 *
 * Run from SpacetimeDB repo root. Writes to templates/<slug>/README.md.
 *
 * Usage: pnpm run generate-readmes (from tools/templates/)
 */

import { readFile, readdir, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(__dirname, '../..');
const TEMPLATES_DIR = path.join(REPO_ROOT, 'templates');
const QUICKSTARTS_DIR = path.join(
  REPO_ROOT,
  'docs/docs/00100-intro/00200-quickstarts'
);
const DOCS_ROOT = path.join(REPO_ROOT, 'docs/docs');

const TEMPLATE_FROM_QUICKSTART_RE = /--template\s+(\S+)/;

/** Parse --template X from quickstart content. Returns template slug or null. */
function parseTemplateFromQuickstart(content: string): string | null {
  const match = content.match(TEMPLATE_FROM_QUICKSTART_RE);
  return match ? match[1] : null;
}

/** Discover template -> quickstart mapping by parsing --template from each quickstart file. */
async function discoverQuickstartMapping(): Promise<Map<string, string>> {
  const map = new Map<string, string>();
  let entries: import('node:fs').Dirent[];
  try {
    entries = await readdir(QUICKSTARTS_DIR, { withFileTypes: true });
  } catch {
    return map;
  }
  const files = entries
    .filter(e => e.isFile() && e.name.endsWith('.md'))
    .map(e => e.name)
    .sort();
  for (const file of files) {
    try {
      const content = await readFile(path.join(QUICKSTARTS_DIR, file), 'utf-8');
      const template = parseTemplateFromQuickstart(content);
      if (template && !map.has(template)) {
        map.set(template, file);
      }
    } catch {
      // skip
    }
  }
  return map;
}

const DOCS_BASE = 'https://spacetimedb.com/docs';

function stripFrontmatterAndImports(content: string): string {
  let out = content.replace(/^---\r?\n[\s\S]*?\r?\n---\r?\n/, '');
  out = out.replace(/^import .+ from ["'][^"']*@site[^"']*["'];\r?\n/gm, '');
  return out.trim();
}

function replaceInstallCardLink(content: string): string {
  return content.replace(
    /<InstallCardLink\s*\/>/g,
    'Install the [SpacetimeDB CLI](https://spacetimedb.com/install) before continuing.'
  );
}

function normalizeStepText(text: string): string {
  return text
    .trim()
    .split('\n')
    .map(line => line.replace(/^\s+/, ''))
    .join('\n');
}

function convertStepByStepToMarkdown(content: string): string {
  const stepRegex =
    /<Step\s+title="([^"]*)">\s*<StepText>\s*([\s\S]*?)\s*<\/StepText>\s*(?:<StepCode>\s*([\s\S]*?)\s*<\/StepCode>)?\s*<\/Step>/g;

  return content.replace(stepRegex, (_, title, stepText, stepCode) => {
    const normalizedText = normalizeStepText(stepText);
    let block = `## ${title}\n\n${normalizedText}\n\n`;
    if (stepCode && stepCode.trim()) {
      block += stepCode.trim() + '\n\n';
    }
    return block;
  });
}

function removeStepByStepWrapper(content: string): string {
  return content.replace(/<StepByStep>\s*([\s\S]*?)\s*<\/StepByStep>/g, '$1');
}

function stripRemainingStepTags(content: string): string {
  let out = content
    .replace(/<StepText>([\s\S]*?)<\/StepText>/g, '$1')
    .replace(/<StepCode>([\s\S]*?)<\/StepCode>/g, '$1')
    .replace(/<Step[^>]*>/g, '')
    .replace(/<\/Step>/g, '')
    .replace(/<\/StepCode>/g, '')
    .replace(/<\/StepText>/g, '');
  return out;
}

function rewriteDocLinks(
  content: string,
  quickstartDir: string,
  docsRoot: string
): string {
  return content.replace(
    /\[([^\]]+)\]\((\.\.\/)*(.+?\.md)(#[\w-]+)?\)/g,
    (_, linkText, parentRefs, docPath, hash) => {
      const relPath = (parentRefs || '') + docPath;
      const resolved = path.resolve(quickstartDir, relPath);
      const relativeToDocs = path
        .relative(docsRoot, resolved)
        .replace(/\\/g, '/');
      const withoutExt = relativeToDocs.replace(/\.md$/, '');
      const slug = withoutExt
        .split('/')
        .map(seg => seg.replace(/^\d+-/, ''))
        .join('/');
      const url = `${DOCS_BASE}/${slug}${hash || ''}`;
      return `[${linkText}](${url})`;
    }
  );
}

function stripLineIndent(md: string): string {
  let inCodeBlock = false;
  return md
    .split('\n')
    .map(line => {
      if (line.startsWith('```')) {
        inCodeBlock = !inCodeBlock;
        return line;
      }
      if (inCodeBlock) return line;
      return line.replace(/^\s+/, '');
    })
    .join('\n');
}

function quickstartMdxToMarkdown(
  mdx: string,
  quickstartDir: string,
  docsRoot: string
): string {
  let md = stripFrontmatterAndImports(mdx);
  md = replaceInstallCardLink(md);
  md = convertStepByStepToMarkdown(md);
  md = removeStepByStepWrapper(md);
  md = stripRemainingStepTags(md);
  md = stripLineIndent(md);
  md = rewriteDocLinks(md, quickstartDir, docsRoot);
  return md.trim() + '\n';
}

/** Resolve quickstart path: override with "/" is relative to DOCS_ROOT, else relative to QUICKSTARTS_DIR. */
function resolveQuickstartPath(override: string): string {
  if (override.includes('/')) {
    return path.join(DOCS_ROOT, override);
  }
  return path.join(QUICKSTARTS_DIR, override);
}

export async function generateTemplateReadmes(): Promise<void> {
  const discovered = await discoverQuickstartMapping();
  let entries: import('node:fs').Dirent[];
  try {
    entries = await readdir(TEMPLATES_DIR, { withFileTypes: true });
  } catch (err) {
    console.warn(`Could not read templates dir: ${TEMPLATES_DIR}`, err);
    return;
  }

  const templateDirs = entries
    .filter(e => e.isDirectory() && !e.name.startsWith('.'))
    .map(e => e.name);

  let generated = 0;
  for (const templateSlug of templateDirs) {
    let quickstartOverride: string | undefined;
    try {
      const metaPath = path.join(TEMPLATES_DIR, templateSlug, '.template.json');
      const metaRaw = await readFile(metaPath, 'utf-8');
      const meta = JSON.parse(metaRaw) as { quickstart?: string };
      quickstartOverride = meta.quickstart;
    } catch {
      // no .template.json or no quickstart field
    }

    let quickstartFullPath: string;
    if (quickstartOverride) {
      const resolved = resolveQuickstartPath(quickstartOverride);
      if (!resolved.startsWith(QUICKSTARTS_DIR)) {
        continue;
      }
      quickstartFullPath = resolved;
    } else {
      const quickstartFile = discovered.get(templateSlug);
      if (!quickstartFile) continue;
      quickstartFullPath = path.join(QUICKSTARTS_DIR, quickstartFile);
    }
    const readmePath = path.join(TEMPLATES_DIR, templateSlug, 'README.md');

    let mdx: string;
    try {
      mdx = await readFile(quickstartFullPath, 'utf-8');
    } catch (err) {
      console.warn(
        `Skipping ${templateSlug}: could not read ${quickstartFullPath}`
      );
      continue;
    }

    const md = quickstartMdxToMarkdown(
      mdx,
      path.dirname(quickstartFullPath),
      DOCS_ROOT
    );
    await writeFile(readmePath, md);
    console.log(`Generated README for ${templateSlug}`);
    generated++;
  }

  console.log(`Generated ${generated} template READMEs`);
}

const isMain =
  import.meta.url === `file://${process.argv[1]}` ||
  fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (isMain) {
  generateTemplateReadmes().catch(err => {
    console.error(err);
    process.exit(1);
  });
}
