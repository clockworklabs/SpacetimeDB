import fs from 'node:fs';
import path from 'node:path';

/**
 * Redirects for URLs that used to resolve on spacetimedb.com and no longer do.
 *
 * `/docs/*` is a static bucket, so each of these becomes a generated HTML stub
 * that bounces the reader on. Two consequences worth knowing: the plugin fails
 * the build if a `to` is not a real route, which is the safety net that keeps
 * this file honest as pages move; and a redirect can never differ from its
 * target only by case, because the stub and the page would be the same file on
 * a case-insensitive filesystem.
 */

export type Redirect = { from: string; to: string };

const DOCS_DIR = path.join(__dirname, 'docs');

/**
 * URLs the pre-Docusaurus docs site served, taken from the `docs/nav.ts` that
 * drove it (deleted in the Docusaurus migration, #3343).
 *
 * Every path here resolved on spacetimedb.com until October 2025, so inbound
 * links, bookmarks and search results still point at them. Paths that came
 * through the migration unchanged — `/bsatn`, `/cli-reference`,
 * `/cli-reference/standalone-config`, `/http/*`, `/sats-json`,
 * `/webassembly-abi`, `/how-to/reject-client-connections` — need no entry.
 */
const legacyRedirects: Redirect[] = [
  // Intro.
  { from: '/index', to: '/' },
  { from: '/getting-started', to: '/' },
  { from: '/ai-chat', to: '/ask-ai/ask-ai' },

  // Deploying.
  { from: '/deploying/maincloud', to: '/how-to/deploy/maincloud' },
  {
    from: '/deploying/spacetimedb-standalone',
    to: '/how-to/deploy/self-hosting',
  },
  // Retired before the migration, but still archived and linked to.
  { from: '/deploying/testnet', to: '/how-to/deploy/maincloud' },

  // Unity tutorial.
  { from: '/unity', to: '/tutorials/unity' },
  { from: '/unity/part-1', to: '/tutorials/unity/part-1' },
  { from: '/unity/part-2', to: '/tutorials/unity/part-2' },
  { from: '/unity/part-3', to: '/tutorials/unity/part-3' },
  { from: '/unity/part-4', to: '/tutorials/unity/part-4' },

  // Unreal tutorial.
  { from: '/unreal', to: '/tutorials/unreal' },
  { from: '/unreal/part-1', to: '/tutorials/unreal/part-1' },
  { from: '/unreal/part-2', to: '/tutorials/unreal/part-2' },
  { from: '/unreal/part-3', to: '/tutorials/unreal/part-3' },
  { from: '/unreal/part-4', to: '/tutorials/unreal/part-4' },
  { from: '/unreal/reference', to: '/clients/unreal' },

  // Server module languages. The per-language module references are gone;
  // that material now lives in the language-agnostic core-concepts pages.
  { from: '/modules', to: '/functions' },
  { from: '/modules/rust', to: '/functions' },
  { from: '/modules/c-sharp', to: '/functions' },
  { from: '/modules/typescript', to: '/functions' },
  { from: '/modules/rust/quickstart', to: '/quickstarts/rust' },
  { from: '/modules/c-sharp/quickstart', to: '/quickstarts/c-sharp' },
  { from: '/modules/typescript/quickstart', to: '/quickstarts/typescript' },

  // Client SDK languages.
  { from: '/sdks', to: '/clients' },
  { from: '/sdks/rust', to: '/clients/rust' },
  { from: '/sdks/c-sharp', to: '/clients/c-sharp' },
  { from: '/sdks/typescript', to: '/clients/typescript' },
  { from: '/sdks/rust/quickstart', to: '/quickstarts/rust' },
  { from: '/sdks/c-sharp/quickstart', to: '/quickstarts/c-sharp' },
  { from: '/sdks/typescript/quickstart', to: '/quickstarts/typescript' },

  // SQL.
  { from: '/sql', to: '/reference/sql' },
  { from: '/sql/pg-wire', to: '/how-to/pg-wire' },

  // Subscriptions.
  { from: '/subscriptions', to: '/clients/subscriptions' },
  { from: '/subscriptions/semantics', to: '/clients/subscriptions/semantics' },

  // Row level security.
  { from: '/rls', to: '/how-to/rls' },

  // How to.
  {
    from: '/how-to/incremental-migrations',
    to: '/databases/incremental-migrations',
  },

  // SpacetimeAuth.
  { from: '/spacetimeauth', to: '/core-concepts/authentication/spacetimeauth' },
  {
    from: '/spacetimeauth/create-project',
    to: '/core-concepts/authentication/spacetimeauth/creating-a-project',
  },
  {
    from: '/spacetimeauth/configure-project',
    to: '/core-concepts/authentication/spacetimeauth/configuring-a-project',
  },
  {
    from: '/spacetimeauth/testing-authentication',
    to: '/core-concepts/authentication/spacetimeauth/testing',
  },
  {
    from: '/spacetimeauth/react-integration',
    to: '/core-concepts/authentication/spacetimeauth/react-integration',
  },

  // HTTP API. The individual pages kept their URLs; only the section index
  // went away.
  { from: '/http', to: '/http/authorization' },

  // Appendix. Its one section documented `#[auto_inc]` sequences.
  { from: '/appendix', to: '/tables/auto-increment' },

  // Never served, but published from this repo. The template README generator
  // built the first three from file paths instead of slugs, and the rest were
  // mistyped in hand-written READMEs. Every project created from a template
  // keeps its own copy of the README, so these links outlive the fix.
  {
    from: '/intro/core-concepts/clients/typescript-reference',
    to: '/clients/typescript',
  },
  {
    from: '/intro/core-concepts/clients/rust-reference',
    to: '/clients/rust',
  },
  {
    from: '/intro/core-concepts/clients/csharp-reference',
    to: '/clients/c-sharp',
  },
  { from: '/reference/cli-reference', to: '/cli-reference' },
  { from: '/reference/sql-reference', to: '/reference/sql' },
  { from: '/sdks/csharp/quickstart', to: '/quickstarts/c-sharp' },
  // Installation is a page on the main site, outside the docs.
  { from: '/install', to: 'https://spacetimedb.com/install' },

  // The Godot tutorial postdates the migration, so these were never live — but
  // they are the obvious guess next to `/unity/part-1` and `/unreal/part-1`,
  // and they are what people try.
  { from: '/godot', to: '/tutorials/godot' },
  { from: '/godot/part-1', to: '/tutorials/godot/part-1' },
  { from: '/godot/part-2', to: '/tutorials/godot/part-2' },
  { from: '/godot/part-3', to: '/tutorials/godot/part-3' },
  { from: '/godot/part-4', to: '/tutorials/godot/part-4' },

  // Parts 3 and 4 were published under `/tutorials/Godot/` until the slug typo
  // was fixed. Those URLs are unrecoverable here — see
  // `rejectCaseOnlyRedirects` — and would have to be handled in front of S3.
];

function markdownFiles(dir: string): string[] {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    const entryPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      return markdownFiles(entryPath);
    }
    return /\.mdx?$/.test(entry.name) ? [entryPath] : [];
  });
}

/**
 * The URL Docusaurus would serve a doc at if it set no `slug`: its path under
 * `docs/`, with the numeric ordering prefixes stripped.
 */
function defaultRoute(file: string): string {
  const rawSegments = path
    .relative(DOCS_DIR, file)
    .replace(/\.mdx?$/, '')
    .split(path.sep);
  const segments = rawSegments.map(segment => segment.replace(/^\d+-/, ''));

  // `index`, `readme` and a file named after its folder all address the folder
  // itself. Docusaurus makes this comparison on the raw filenames, before the
  // ordering prefixes come off, so `00000-index.md` is not a folder index.
  const leaf = rawSegments.at(-1)!.toLowerCase();
  const parent = rawSegments.at(-2)?.toLowerCase();
  if (leaf === 'index' || leaf === 'readme' || leaf === parent) {
    segments.pop();
  }

  return `/${segments.join('/')}`;
}

function frontMatterSlug(file: string): string | undefined {
  const frontMatter = /^---\r?\n([\s\S]*?)\r?\n---/.exec(
    fs.readFileSync(file, 'utf8')
  )?.[1];
  return /^slug:\s*(\S+)\s*$/m
    .exec(frontMatter ?? '')?.[1]
    .replace(/^['"]|['"]$/g, '');
}

/**
 * Nearly every doc sets an explicit `slug`, which flattens the numbered folder
 * structure out of its URL: `docs/00100-intro/00300-tutorials/00100-chat-app.md`
 * is served at `/tutorials/chat-app`, not `/intro/tutorials/chat-app`. The
 * folder-shaped path is still what the source tree looks like, so it is what
 * anyone reading it will reasonably guess. Point each one at the slug the page
 * actually uses.
 */
const structuralRedirects: Redirect[] = markdownFiles(DOCS_DIR)
  .flatMap(file => {
    const from = defaultRoute(file);
    const to = frontMatterSlug(file);
    // A doc with no slug already lives at its folder-shaped path, and one whose
    // slug *is* that path would redirect to itself — which the plugin rejects,
    // since writing the redirect would overwrite the page.
    return to && to !== from ? [{ from, to }] : [];
  })
  // Deterministic across filesystems; the plugin does not care about order.
  .sort((a, b) => a.from.localeCompare(b.from));

/**
 * A redirect whose source differs from its target only by case cannot be
 * emitted: on a case-insensitive filesystem the generated redirect file and the
 * real page are the same file, and the build aborts with an unexplained "not
 * supposed to override existing files". Fail here, where the message can say
 * what to do about it, rather than on the next Mac to run a build.
 */
function rejectCaseOnlyRedirects(all: Redirect[]): Redirect[] {
  const caseOnly = all.filter(
    ({ from, to }) => from !== to && from.toLowerCase() === to.toLowerCase()
  );
  if (caseOnly.length > 0) {
    throw new Error(
      'These redirects differ from their target only by case, which cannot be ' +
        'served from a static build. Rename the source file to match the slug ' +
        'instead:\n' +
        caseOnly.map(({ from, to }) => `  ${from} -> ${to}`).join('\n')
    );
  }
  return all;
}

export const redirects: Redirect[] = rejectCaseOnlyRedirects([
  ...legacyRedirects,
  ...structuralRedirects,
]);
