#!/usr/bin/env node

import fs from 'node:fs';
import path from 'node:path';

const DOCS_ROOT = path.resolve(import.meta.dirname, '..');
const SRC_DIR = path.join(DOCS_ROOT, 'docs');
const DEST_DIR = path.join(DOCS_ROOT, 'content', 'docs');

// ─── Explicit slug → destination mapping ───────────────────────────────────────
// Key: relative path from SRC_DIR, Value: destination relative to DEST_DIR
const SLUG_MAP = new Map([
  ['00100-intro/00100-getting-started/00100-getting-started.md', 'index.mdx'],
  ['00100-intro/00100-getting-started/00200-what-is-spacetimedb.md', 'intro/what-is-spacetimedb.mdx'],
  ['00100-intro/00100-getting-started/00250-zen-of-spacetimedb.md', 'intro/zen.mdx'],
  ['00100-intro/00100-getting-started/00300-language-support.md', 'intro/language-support.mdx'],
  ['00100-intro/00100-getting-started/00400-key-architecture.md', 'intro/key-architecture.mdx'],
  ['00100-intro/00100-getting-started/00500-faq.md', 'intro/faq.mdx'],
  ['00100-intro/00200-quickstarts/00100-react.md', 'quickstarts/react.mdx'],
  ['00100-intro/00200-quickstarts/00150-vue.md', 'quickstarts/vue.mdx'],
  ['00100-intro/00200-quickstarts/00160-svelte.md', 'quickstarts/svelte.mdx'],
  ['00100-intro/00200-quickstarts/00400-typescript.md', 'quickstarts/typescript.mdx'],
  ['00100-intro/00200-quickstarts/00500-rust.md', 'quickstarts/rust.mdx'],
  ['00100-intro/00200-quickstarts/00600-c-sharp.md', 'quickstarts/c-sharp.mdx'],
  ['00100-intro/00300-tutorials/00100-chat-app.md', 'tutorials/chat-app.mdx'],
  ['00100-intro/00300-tutorials/00300-unity-tutorial/index.md', 'tutorials/unity/index.mdx'],
  ['00100-intro/00300-tutorials/00300-unity-tutorial/00200-part-1.md', 'tutorials/unity/part-1.mdx'],
  ['00100-intro/00300-tutorials/00300-unity-tutorial/00300-part-2.md', 'tutorials/unity/part-2.mdx'],
  ['00100-intro/00300-tutorials/00300-unity-tutorial/00400-part-3.md', 'tutorials/unity/part-3.mdx'],
  ['00100-intro/00300-tutorials/00300-unity-tutorial/00500-part-4.md', 'tutorials/unity/part-4.mdx'],
  ['00100-intro/00300-tutorials/00400-unreal-tutorial/index.md', 'tutorials/unreal/index.mdx'],
  ['00100-intro/00300-tutorials/00400-unreal-tutorial/00200-part-1.md', 'tutorials/unreal/part-1.mdx'],
  ['00100-intro/00300-tutorials/00400-unreal-tutorial/00300-part-2.md', 'tutorials/unreal/part-2.mdx'],
  ['00100-intro/00300-tutorials/00400-unreal-tutorial/00400-part-3.md', 'tutorials/unreal/part-3.mdx'],
  ['00100-intro/00300-tutorials/00400-unreal-tutorial/00500-part-4.md', 'tutorials/unreal/part-4.mdx'],
  ['00200-core-concepts/00000-index.md', 'core-concepts/index.mdx'],
  ['00200-core-concepts/00100-databases.md', 'databases/index.mdx'],
  ['00200-core-concepts/00100-databases/00100-transactions-atomicity.md', 'databases/transactions-atomicity.mdx'],
  ['00200-core-concepts/00100-databases/00200-spacetime-dev.md', 'databases/developing.mdx'],
  ['00200-core-concepts/00100-databases/00300-spacetime-publish.md', 'databases/building-publishing.mdx'],
  ['00200-core-concepts/00100-databases/00500-cheat-sheet.md', 'databases/cheat-sheet.mdx'],
  ['00200-core-concepts/00100-databases/00500-migrations/00200-automatic-migrations.md', 'databases/automatic-migrations.mdx'],
  ['00200-core-concepts/00100-databases/00500-migrations/00300-incremental-migrations.md', 'databases/incremental-migrations.mdx'],
  ['00200-core-concepts/00200-functions.md', 'functions/index.mdx'],
  ['00200-core-concepts/00200-functions/00300-reducers/00300-reducers.md', 'functions/reducers/index.mdx'],
  ['00200-core-concepts/00200-functions/00300-reducers/00400-reducer-context.md', 'functions/reducers/reducer-context.mdx'],
  ['00200-core-concepts/00200-functions/00300-reducers/00500-lifecycle.md', 'functions/reducers/lifecycle.mdx'],
  ['00200-core-concepts/00200-functions/00300-reducers/00600-error-handling.md', 'functions/reducers/error-handling.mdx'],
  ['00200-core-concepts/00200-functions/00400-procedures.md', 'functions/procedures.mdx'],
  ['00200-core-concepts/00200-functions/00500-views.md', 'functions/views.mdx'],
  ['00200-core-concepts/00300-tables.md', 'tables/index.mdx'],
  ['00200-core-concepts/00300-tables/00200-column-types.md', 'tables/column-types.mdx'],
  ['00200-core-concepts/00300-tables/00210-file-storage.md', 'tables/file-storage.mdx'],
  ['00200-core-concepts/00300-tables/00230-auto-increment.md', 'tables/auto-increment.mdx'],
  ['00200-core-concepts/00300-tables/00240-constraints.md', 'tables/constraints.mdx'],
  ['00200-core-concepts/00300-tables/00250-default-values.md', 'tables/default-values.mdx'],
  ['00200-core-concepts/00300-tables/00300-indexes.md', 'tables/indexes.mdx'],
  ['00200-core-concepts/00300-tables/00400-access-permissions.md', 'tables/access-permissions.mdx'],
  ['00200-core-concepts/00300-tables/00500-schedule-tables.md', 'tables/schedule-tables.mdx'],
  ['00200-core-concepts/00300-tables/00600-performance.md', 'tables/performance.mdx'],
  ['00200-core-concepts/00400-subscriptions.md', 'subscriptions/index.mdx'],
  ['00200-core-concepts/00400-subscriptions/00200-subscription-semantics.md', 'subscriptions/semantics.mdx'],
  ['00200-core-concepts/00600-client-sdk-languages.md', 'sdks/index.mdx'],
  ['00200-core-concepts/00600-client-sdk-languages/00200-codegen.md', 'sdks/codegen.mdx'],
  ['00200-core-concepts/00600-client-sdk-languages/00300-connection.md', 'sdks/connection.mdx'],
  ['00200-core-concepts/00600-client-sdk-languages/00400-sdk-api.md', 'sdks/api.mdx'],
  ['00200-core-concepts/00600-client-sdk-languages/00500-rust-reference.md', 'sdks/rust.mdx'],
  ['00200-core-concepts/00600-client-sdk-languages/00600-csharp-reference.md', 'sdks/c-sharp.mdx'],
  ['00200-core-concepts/00600-client-sdk-languages/00700-typescript-reference.md', 'sdks/typescript.mdx'],
  ['00200-core-concepts/00600-client-sdk-languages/00800-unreal-reference.md', 'sdks/unreal.mdx'],
  ['00300-resources/00000-index.md', 'resources/index.mdx'],
  ['00300-resources/00100-how-to/00100-deploy/00100-maincloud.md', 'how-to/deploy/maincloud.mdx'],
  ['00300-resources/00100-how-to/00100-deploy/00200-self-hosting.md', 'how-to/deploy/self-hosting.mdx'],
  ['00300-resources/00100-how-to/00200-pg-wire.md', 'how-to/pg-wire.mdx'],
  ['00300-resources/00100-how-to/00300-logging.md', 'how-to/logging.mdx'],
  ['00300-resources/00100-how-to/00400-row-level-security.md', 'how-to/rls.mdx'],
  ['00300-resources/00100-how-to/00500-reject-client-connections.md', 'how-to/reject-client-connections.mdx'],
  ['00300-resources/00200-reference/00100-cli-reference/00100-cli-reference.md', 'cli-reference/index.mdx'],
  ['00300-resources/00200-reference/00100-cli-reference/00200-standalone-config.md', 'cli-reference/standalone-config.mdx'],
  ['00300-resources/00200-reference/00200-http-api/00100-authorization.md', 'http/authorization.mdx'],
  ['00300-resources/00200-reference/00200-http-api/00200-identity.md', 'http/identity.mdx'],
  ['00300-resources/00200-reference/00200-http-api/00300-database.md', 'http/database.mdx'],
  ['00300-resources/00200-reference/00300-internals/00100-module-abi-reference.md', 'webassembly-abi.mdx'],
  ['00300-resources/00200-reference/00300-internals/00200-sats-json.md', 'sats-json.mdx'],
  ['00300-resources/00200-reference/00300-internals/00300-bsatn.md', 'bsatn.mdx'],
  ['00300-resources/00200-reference/00400-sql-reference.md', 'reference/sql.mdx'],
]);

// Files without slugs (authentication section)
const NO_SLUG_MAP = new Map([
  ['00200-core-concepts/00500-authentication.md', 'core-concepts/authentication/index.mdx'],
  ['00200-core-concepts/00500-authentication/00100-spacetimeauth/index.md', 'core-concepts/authentication/spacetimeauth/index.mdx'],
  ['00200-core-concepts/00500-authentication/00100-spacetimeauth/00200-creating-a-project.md', 'core-concepts/authentication/spacetimeauth/creating-a-project.mdx'],
  ['00200-core-concepts/00500-authentication/00100-spacetimeauth/00300-configuring-a-project.md', 'core-concepts/authentication/spacetimeauth/configuring-a-project.mdx'],
  ['00200-core-concepts/00500-authentication/00100-spacetimeauth/00400-testing.md', 'core-concepts/authentication/spacetimeauth/testing.mdx'],
  ['00200-core-concepts/00500-authentication/00100-spacetimeauth/00500-react-integration.md', 'core-concepts/authentication/spacetimeauth/react-integration.mdx'],
  ['00200-core-concepts/00500-authentication/00200-Auth0.md', 'core-concepts/authentication/auth0.mdx'],
  ['00200-core-concepts/00500-authentication/00300-Clerk.md', 'core-concepts/authentication/clerk.mdx'],
  ['00200-core-concepts/00500-authentication/00500-usage.md', 'core-concepts/authentication/usage.mdx'],
]);

// Ask-ai file
const ASK_AI_MAP = new Map([
  ['00000-ask-ai/00100-ask-ai.mdx', 'ask-ai.mdx'],
]);

// Merge all mappings
const ALL_MAPPINGS = new Map([...SLUG_MAP, ...NO_SLUG_MAP, ...ASK_AI_MAP]);

// Build a reverse lookup: old relative path -> new relative path (for link rewriting)
// Also build: old relative path from SRC_DIR -> new destination dir relative to DEST_DIR
const OLD_PATH_TO_NEW = new Map();
for (const [oldRel, newRel] of ALL_MAPPINGS) {
  OLD_PATH_TO_NEW.set(oldRel, newRel);
}

// ─── Frontmatter parsing ────────────────────────────────────────────────────

function parseFrontmatter(content) {
  const match = content.match(/^---\r?\n([\s\S]*?)\r?\n---/);
  if (!match) return { frontmatter: {}, body: content, raw: '' };

  const raw = match[1];
  const body = content.slice(match[0].length);
  const frontmatter = {};

  for (const line of raw.split('\n')) {
    const colonIdx = line.indexOf(':');
    if (colonIdx === -1) continue;
    const key = line.slice(0, colonIdx).trim();
    let value = line.slice(colonIdx + 1).trim();
    // Strip quotes
    if ((value.startsWith("'") && value.endsWith("'")) ||
        (value.startsWith('"') && value.endsWith('"'))) {
      value = value.slice(1, -1);
    }
    frontmatter[key] = value;
  }

  return { frontmatter, body, raw };
}

function buildFrontmatter(fm) {
  const lines = ['---'];
  for (const [key, value] of Object.entries(fm)) {
    if (value === true) {
      lines.push(`${key}: true`);
    } else if (value === false) {
      lines.push(`${key}: false`);
    } else if (value === 'null' || value === null) {
      // skip null values
    } else {
      // Quote values that contain special characters
      if (typeof value === 'string' && (value.includes(':') || value.includes('#') || value.includes("'") || value.includes('"'))) {
        lines.push(`${key}: '${value.replace(/'/g, "''")}'`);
      } else {
        lines.push(`${key}: ${value}`);
      }
    }
  }
  lines.push('---');
  return lines.join('\n');
}

// ─── Content transformations ────────────────────────────────────────────────

function transformImports(content) {
  // Replace @theme/Tabs and @theme/TabItem with @/components/Tabs
  content = content.replace(
    /import\s+Tabs\s+from\s+['"]@theme\/Tabs['"];?/g,
    "import { Tabs, Tab } from 'fumadocs-ui/components/tabs';"
  );
  content = content.replace(
    /import\s+TabItem\s+from\s+['"]@theme\/TabItem['"];?\n?/g,
    ''
  );

  // Replace @site/src/components/ with @/components/
  content = content.replace(/@site\/src\/components\//g, '@/components/');

  // Replace @site/static/images/ with /images/
  content = content.replace(/@site\/static\/images\//g, '/images/');

  // Replace @docusaurus/Link with next/link
  content = content.replace(/@docusaurus\/Link/g, 'next/link');

  return content;
}

function transformTabItems(content) {
  // Convert <TabItem label="X"> to <Tab value="X">
  content = content.replace(/<TabItem\s+label="([^"]+)"[^>]*>/g, '<Tab value="$1">');
  content = content.replace(/<TabItem\s+value="([^"]+)"[^>]*>/g, '<Tab value="$1">');
  content = content.replace(/<\/TabItem>/g, '</Tab>');
  return content;
}

function transformHtmlComments(content) {
  // Convert <!-- ... --> to {/* ... */} for MDX compatibility
  content = content.replace(/<!--([\s\S]*?)-->/g, '{/*$1*/}');
  return content;
}

function cleanFrontmatter(fm) {
  const cleaned = {};
  for (const [key, value] of Object.entries(fm)) {
    // Remove these keys
    if (['slug', 'sidebar_label', 'pagination_next', 'pagination_prev'].includes(key)) {
      continue;
    }
    cleaned[key] = value;
  }
  return cleaned;
}

// ─── Link rewriting ─────────────────────────────────────────────────────────

// Strip numeric prefixes from a path segment: "00200-foo" -> "foo"
function stripNumericPrefix(segment) {
  return segment.replace(/^\d+-/, '');
}

function rewriteRelativeLinks(content, srcRelPath, destRelPath) {
  // Match markdown links with relative paths: [text](./path) or [text](../path)
  return content.replace(
    /\]\((\.\.[^)]*|\.\/[^)]*)\)/g,
    (match, linkPath) => {
      // Separate anchor from path
      const [pathPart, anchor] = linkPath.split('#');

      // Resolve the link relative to the source file's directory
      const srcDir = path.dirname(path.join(SRC_DIR, srcRelPath));
      const resolvedOldPath = path.resolve(srcDir, pathPart);
      const resolvedOldRel = path.relative(SRC_DIR, resolvedOldPath);

      // Look up the destination for the linked file
      const destForLinked = ALL_MAPPINGS.get(resolvedOldRel);

      if (destForLinked) {
        // Compute relative path from the current destination file to the linked destination
        const destDir = path.dirname(path.join(DEST_DIR, destRelPath));
        const linkedDest = path.join(DEST_DIR, destForLinked);
        let newRelLink = path.relative(destDir, linkedDest);

        // Remove .mdx extension for clean links
        newRelLink = newRelLink.replace(/\.mdx$/, '');
        // Remove trailing /index for directory index pages
        newRelLink = newRelLink.replace(/\/index$/, '');

        // Ensure it starts with ./
        if (!newRelLink.startsWith('.') && !newRelLink.startsWith('/')) {
          newRelLink = './' + newRelLink;
        }

        const anchorPart = anchor ? '#' + anchor : '';
        return `](${newRelLink}${anchorPart})`;
      }

      // If we can't find a mapping, just strip numeric prefixes from the path
      const stripped = pathPart.replace(/\d+-/g, '');
      // Also strip .md extension
      const cleanPath = stripped.replace(/\.md$/, '').replace(/\.mdx$/, '');
      const anchorPart = anchor ? '#' + anchor : '';
      return `](${cleanPath}${anchorPart})`;
    }
  );
}

// ─── Main migration ─────────────────────────────────────────────────────────

function migrate() {
  let migratedCount = 0;
  const results = [];

  for (const [srcRel, destRel] of ALL_MAPPINGS) {
    const srcPath = path.join(SRC_DIR, srcRel);
    const destPath = path.join(DEST_DIR, destRel);

    if (!fs.existsSync(srcPath)) {
      console.error(`WARNING: Source file not found: ${srcRel}`);
      continue;
    }

    let content = fs.readFileSync(srcPath, 'utf-8');
    const { frontmatter, body, raw } = parseFrontmatter(content);

    // Clean frontmatter
    const cleanedFm = cleanFrontmatter(frontmatter);

    // Build new content
    let newContent;
    if (Object.keys(cleanedFm).length > 0) {
      newContent = buildFrontmatter(cleanedFm) + body;
    } else if (raw) {
      // Had frontmatter but all keys were removed
      newContent = '---\n---' + body;
    } else {
      // No frontmatter at all (like authentication.md)
      newContent = content;
    }

    // Apply content transformations
    newContent = transformImports(newContent);
    newContent = transformTabItems(newContent);
    newContent = transformHtmlComments(newContent);
    newContent = rewriteRelativeLinks(newContent, srcRel, destRel);

    // Ensure destination directory exists
    const destDir = path.dirname(destPath);
    fs.mkdirSync(destDir, { recursive: true });

    // Write file
    fs.writeFileSync(destPath, newContent, 'utf-8');
    migratedCount++;
    results.push(`  ${srcRel} -> ${destRel}`);
  }

  return { migratedCount, results };
}

// ─── Run ────────────────────────────────────────────────────────────────────

console.log('Starting content migration...\n');
console.log(`Source: ${SRC_DIR}`);
console.log(`Destination: ${DEST_DIR}\n`);

const { migratedCount, results } = migrate();

console.log('Migration results:');
for (const r of results) {
  console.log(r);
}
console.log(`\nTotal files migrated: ${migratedCount}`);

// Verify no source files were missed
const allSourceFiles = [];
function walkDir(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walkDir(fullPath);
    } else if (entry.name.endsWith('.md') || entry.name.endsWith('.mdx')) {
      allSourceFiles.push(path.relative(SRC_DIR, fullPath));
    }
  }
}
walkDir(SRC_DIR);

const unmapped = allSourceFiles.filter(f => !ALL_MAPPINGS.has(f));
if (unmapped.length > 0) {
  console.log('\nWARNING: Unmapped source files:');
  for (const f of unmapped) {
    console.log(`  ${f}`);
  }
}

// Verify destination files
const destFiles = [];
function walkDest(dir) {
  if (!fs.existsSync(dir)) return;
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walkDest(fullPath);
    } else {
      destFiles.push(path.relative(DEST_DIR, fullPath));
    }
  }
}
walkDest(DEST_DIR);

console.log(`\nDestination files (${destFiles.length}):`);
for (const f of destFiles.sort()) {
  console.log(`  ${f}`);
}
