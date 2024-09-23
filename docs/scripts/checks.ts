import { create, insert, search } from '@orama/orama';
import kleur from 'kleur';
import {
  Marked,
  type MarkedExtension,
  type Renderer,
  type TokenizerObject,
  type Tokens,
} from 'marked';
import { readdir, readFile } from 'node:fs/promises';

//////////////////////////////////////////////// !FLAGS ////////////////////////////////////////////////
// If you want to disable any of these, set them to false
const CHECK_EXTERNAL_LINKS = true;
const PRINT_ERRORS = true;
///////////////////////////////////////////////////////////////////////////////////////////////////

const data = await gatherData();

const errors = new Map<
  string,
  Set<{
    file: string;
    line: number;
    message: string;
    suggestion?: string | null;
  }>
>([]);

for (const [slug] of data) {
  errors.set(slug, new Set([]));
}

await Promise.all([checkHeadingsOrder(), checkLinks()]);

// Cleanup errors. If a slug has an empty set, then remove it from the map.
for (const [slug, slugErrors] of errors) {
  if (slugErrors.size === 0) {
    errors.delete(slug);
  }
}

// Count total number of errors
const totalErrors = Array.from(errors.values()).reduce(
  (acc, cur) => acc + cur.size,
  0
);

if (PRINT_ERRORS)
  if (errors.size !== 0) {
    console.log(kleur.red().bold(`${totalErrors} ERRORS`));
    for (const [slug, slugErrors] of errors) {
      console.log(kleur.dim().bold(slug.padStart(40, '-').padEnd(60, '-')));
      for (const error of slugErrors) {
        console.log(
          kleur
            .yellow()
            .bold(
              `  ${new URL(`../content/docs/${error.file}`, import.meta.url).pathname}:${error.line}`
            )
        );
        console.log(kleur.red().bold(`    ${error.message}`));
        if (error.suggestion) {
          console.log(
            kleur.green().bold(`    Did you mean: ${error.suggestion}`)
          );
        }
        console.log();
      }
    }
    console.log(kleur.red().bold(`${totalErrors} ERRORS`));

    throw new Error('');
  } else {
    console.log(
      kleur
        .green()
        .bold('✅✅✅✅✅✅✅✅✅ No errors found! ✅✅✅✅✅✅✅✅✅')
    );
  }

async function gatherData() {
  const dirs = await readdir(new URL('../content/docs', import.meta.url));

  const data: Map<
    string,
    {
      path: string;
      title: string;
      navTitle: string;
      content: string;
      raw: string;
    }
  > = new Map();

  for (const dir of dirs) {
    const dir_contents = await readdir(
      new URL(`../content/docs/${dir}`, import.meta.url)
    );

    for (const file of dir_contents) {
      if (file.endsWith('meta.json')) continue;

      const file_contents = await readFile(
        new URL(`../content/docs/${dir}/${file}`, import.meta.url),
        'utf8'
      );

      const { metadata, body } = extractFrontmatter(file_contents);

      const slug = `${dir.slice(3)}/${file.slice(3).slice(0, -3)}`;
      data.set(slug, {
        path: dir + '/' + file,
        title: metadata.title,
        navTitle: metadata.navTitle,
        content: body,
        raw: file_contents,
      });
    }
  }

  return data;
}

function extractFrontmatter(markdown: string) {
  const match = /---\r?\n([\s\S]+?)\r?\n---/.exec(markdown);
  if (!match) return { metadata: {}, body: markdown };

  const frontmatter = match[1];
  const body = markdown.slice(match[0].length);

  const metadata: Record<string, string> = {};
  frontmatter.split('\n').forEach(pair => {
    const i = pair.indexOf(':');
    metadata[pair.slice(0, i).trim()] = removeQuotes(pair.slice(i + 1).trim());
  });

  return { metadata, body };
}

function removeQuotes(str: string) {
  return str.replace(/(^["']|["']$)/g, '');
}

async function transform(
  markdown: string,
  renderer: Partial<Renderer> = {},
  extension?: MarkedExtension
) {
  const tokenizer: TokenizerObject = {
    url(src) {
      // if `src` is a package version string, eg: adapter-auto@1.2.3
      // do not tokenize it as email
      if (/@\d+\.\d+\.\d+/.test(src)) {
        return undefined;
      }
      // else, use the default tokenizer behavior
      return false;
    },
  };

  const marked = new Marked({
    renderer,
    tokenizer,
  });

  if (extension) marked.use(extension);

  return await marked.parse(markdown);
}

async function checkLinks() {
  const headingsOnPages = new Map<string, Set<string>>();

  // Gather all the headings
  for (const [slug, { content }] of data) {
    const headings: string[] = [];

    // this is a bit hacky, but it allows us to prevent type declarations
    // from linking to themselves
    let current = '';

    headingsOnPages.set(slug, new Set());
    const onPageHeadings = headingsOnPages.get(slug)!;

    await transform(content, {
      heading({ raw, depth }) {
        const title = raw
          .replace(/<\/?code>/g, '')
          .replace(/&quot;/g, '"')
          .replace(/&lt;/g, '<')
          .replace(/&gt;/g, '>');

        current = title;

        const normalized = normalizeSlugify(raw);

        headings[depth - 1] = normalized;
        headings.length = depth;

        const slug = headings.filter(Boolean).join('-');
        onPageHeadings.add(slug);

        return '';
      },
    });
  }

  const db = await create({
    schema: {
      slug: 'string',
      hash: 'string',
      terms: 'string[]',
    },
    components: {
      tokenizer: {
        stemming: true,
      },
    },
  });

  // Populate the database with all the headings
  for (const [slug, onPageHeadings] of headingsOnPages) {
    for (const hash of onPageHeadings) {
      // @ts-ignore
      await insert(db, {
        slug,
        hash,
        terms: [...slug.split('/'), ...hash.split(/[^a-zA-Z0-9]+/)],
      });
    }
  }

  // Now compare links. What I am looking for:
  // Links starting with # are same-page links, so go through each link on every document and make sure the link is in the set of the page
  // Links starting with /docs/* should be compared properly to the set of headings on the page. if they end with #something, then copare the hash link as well.
  // If the link is not in the set of headings on the page, then it is an error.
  for (const [slug, { raw, path }] of data) {
    const slugErrors = errors.get(slug)!;
    const lines = raw.split('\n');

    const linksToCheck = new Set<string>();

    await transform(
      raw,
      {},
      {
        async: true,
        async walkTokens(token) {
          if (token.type !== 'link') return;

          const { href } = token as Tokens.Link;

          if (href.startsWith('#')) {
            const hash = href.slice(1);
            if (!headingsOnPages.get(slug)!.has(hash)) {
              // Search for the closest heading on the page
              const results = await search(db, {
                term: hash.split(/[^a-zA-Z0-9]+/).join(' '),
                properties: ['terms'],
                where: {
                  // @ts-ignore
                  slug,
                },
                limit: 1,
                tolerance: 1,
              });

              slugErrors.add({
                message: `Link to #${hash} on page ${slug} does not exist`,
                file: path,
                line: lines.findIndex(line => line.includes(href)) + 1,
                suggestion:
                  results.count > 0
                    ? // @ts-ignore
                      '#' + results.hits[0].document.hash
                    : null,
              });
            }
          } else if (href.startsWith('/docs')) {
            //  Should start with /docs. Then compare, including any hash it might have. Examples: /docs/data-format/bsatn or /docs/introduction/getting-started#some-heading
            const link = href.slice(1);
            const slug = link.slice(5).split('#')[0];
            const hashIfThere = link.includes('#')
              ? link.slice(link.indexOf('#') + 1)
              : null;

            if (
              !headingsOnPages.has(slug) ||
              (headingsOnPages.has(slug) &&
                hashIfThere &&
                !headingsOnPages.get(slug)!.has(hashIfThere))
            ) {
              const results = await search(db, {
                term:
                  slug.split(/[^a-zA-Z0-9]+/).join(' ') +
                  ' ' +
                  (hashIfThere
                    ? hashIfThere.split(/[^a-zA-Z0-9]+/).join(' ')
                    : ''),
                properties: ['terms'],
                limit: 1,
                tolerance: 1,
              });

              slugErrors.add({
                message: `Link to ${link} on page ${slug.split('#')[0]} does not exist`,
                file: path,
                line: lines.findIndex(line => line.includes(href)) + 1,
                suggestion:
                  results.count > 0
                    ? '/docs/' +
                      // @ts-ignore
                      results.hits[0].document.slug +
                      // @ts-ignore
                      (hashIfThere ? '#' + results.hits[0].document.hash : '')
                    : null,
              });
            }
          } else if (/^https?:\/\//.test(href)) {
            // If the link is an external URL, then add it to the link queue
            linksToCheck.add(href);
          }
        },
      }
    );

    // Check links to external URLs
    if (CHECK_EXTERNAL_LINKS) {
      if (linksToCheck.size === 0)
        console.log(
          kleur.bgYellow().bold(`Skipping ${slug}: No external links found`)
        );
      else console.log(kleur.bgCyan().bold(`Checking ${slug}`) + '\n');

      for (const link of linksToCheck) {
        console.log(kleur.dim().bold(`    ${link}`));
        const response = await fetch(link, {
          // Required as crates.io doesn't allow non browser user agents
          headers: {
            'User-Agent':
              'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36',
            Accept:
              'text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,image/apng,*/*;q=0.8',
            'Accept-Language': 'en-US,en;q=0.9',
          },
        });
        if (!response.ok) {
          slugErrors.add({
            message: `External: Link to ${link} is ${response.status}:${response.statusText}`,
            file: path,
            line: lines.findIndex(line => line.includes(link)) + 1,
          });
        }
      }
      console.log('\n');
    }
  }
}

function slugify(title: string) {
  return title
    .toLowerCase()
    .replace(/&#39;/g, '')
    .replace(/&lt;/g, '')
    .replace(/&gt;/g, '')
    .replace(/[^a-z0-9-$]/g, '-')
    .replace(/-{2,}/g, '-')
    .replace(/^-/, '')
    .replace(/-$/, '');
}

function removeMarkdown(markdown: string) {
  return markdown
    .replace(/\*\*(.+?)\*\*/g, '$1') // bold
    .replace(/(?<=\s)_(.+?)_(?=\s)/g, '$1') // Italics
    .replace(/\*(.+?)\*/g, '$1') // Italics
    .replace(/`(.+?)`/g, '$1') // Inline code
    .replace(/~~(.+?)~~/g, '$1') // Strikethrough
    .replace(/\[(.+?)\]\(.+?\)/g, '$1') // Link
    .replace(/\n/g, ' ') // New line
    .replace(/ {2,}/g, ' ')
    .trim();
}

function removeHTMLEntities(html: string) {
  return html.replace(/&.+?;/g, '');
}

function normalizeSlugify(str: string) {
  return slugify(removeHTMLEntities(removeMarkdown(str))).replace(
    /(<([^>]+)>)/gi,
    ''
  );
}

async function checkHeadingsOrder() {
  for (const [slug, { raw, path }] of Object.entries(data)) {
    const slugErrors = errors.get(slug)!;

    const lines = raw.split('\n');

    const root = {
      title: 'Root',
      slug: 'root',
      sections: [],
      breadcrumbs: [''],
      text: '',
    };
    let currentNodes = [root];

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];

      const match = line.match(/^(#{1,6})\s(.*)/);

      if (match) {
        const level = match[1].length - 2;
        if (level === -1) {
          slugErrors.add({
            message: 'Heading level 1',
            file: path,
            line: i,
          });
          continue;
        }

        const text = match[2];

        if (level >= currentNodes.length) {
          slugErrors.add({
            message:
              'It seems you are using non-consecutive headings for ' +
              text +
              ' (e.g ### after # instead of ## > ###) in your markdown file. Please fix it and try again.',
            file: path,
            line: i,
          });
          continue;
        }

        const newNode = {
          title: text,
          slug,
          sections: [],
          breadcrumbs: [...currentNodes[level].breadcrumbs, text],
          text: '',
        };

        // Add the new node to the tree
        const sections = currentNodes[level].sections as any[];
        if (!sections) throw new Error(`Could not find section ${level}`);
        sections.push(newNode);

        // Prepare for potential children of the new node
        currentNodes = currentNodes.slice(0, level + 1);
        currentNodes.push(newNode);
      } else if (line.trim() !== '') {
        // Add non-heading line to the text of the current section
        currentNodes[currentNodes.length - 1].text += line + '\n';
      }
    }
  }
}
