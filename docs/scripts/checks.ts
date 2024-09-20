import { Marked, Renderer, TokenizerObject } from 'marked';
import { readdir, readFile } from 'node:fs/promises';
import kleur from 'kleur';

const data = await gatherData();

const errors = new Map<
  string,
  Set<{ file: string; line: number; message: string }>
>([]);

for (const [slug] of Object.entries(data)) {
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
      console.log();
    }
  }
  console.log(kleur.red().bold(`${totalErrors} ERRORS`));
} else {
  console.log(
    kleur.green().bold('✅✅✅✅✅✅✅✅✅ No errors found! ✅✅✅✅✅✅✅✅✅')
  );
}

async function gatherData() {
  const dirs = await readdir(new URL('../content/docs', import.meta.url));

  const data: Record<
    string,
    {
      path: string;
      title: string;
      navTitle: string;
      content: string;
      raw: string;
    }
  > = {};

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
      data[slug] = {
        path: dir + '/' + file,
        title: metadata.title,
        navTitle: metadata.navTitle,
        content: body,
        raw: file_contents,
      };
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

async function transform(markdown: string, renderer: Partial<Renderer> = {}) {
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

  return await marked.parse(markdown);
}

async function checkLinks() {
  const headingsOnPages = new Map<string, Set<string>>();

  // Gather all the headings
  for (const [slug, { content }] of Object.entries(data)) {
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

  // Now compare links. What I am looking for:
  // Links starting with # are same-page links, so go through each link on every document and make sure the link is in the set of the page
  // Links starting with /docs/* should be compared properly to the set of headings on the page. if they end with #something, then copare the hash link as well.
  // Links starting with ./ or ../ should be resolved properly based opn current page and then compared. It should resolve to the above /docs link or so.
  // If the link is not in the set of headings on the page, then it is an error.
  for (const [slug, { raw, path }] of Object.entries(data)) {
    const slugErrors = errors.get(slug)!;
    const lines = raw.split('\n');

    await transform(raw, {
      link({ href }) {
        if (href.startsWith('#')) {
          const link = href.slice(1);
          if (!headingsOnPages.get(slug)!.has(link)) {
            slugErrors.add({
              message: `Link to #${link} on page ${slug} does not exist`,
              file: path,
              line: lines.findIndex(line => line.includes(href)) + 1,
            });
          }
        } else if (href.startsWith('/docs')) {
          //  Should start with /docs. Then compare, including any hash it might have. Examples: /docs/data-format/bsatn or /docs/introduction/getting-started#some-heading
          const link = href.slice(1);
          const slug = link.slice(5);
          const hashIfThere = slug.includes('#')
            ? slug.slice(slug.indexOf('#'))
            : null;

          if (!headingsOnPages.has(slug)) {
            slugErrors.add({
              message: `Link to ${link} on page ${slug} does not exist`,
              file: path,
              line: lines.findIndex(line => line.includes(href)) + 1,
            });
          } else {
            if (hashIfThere) {
              if (!headingsOnPages.get(slug)!.has(hashIfThere)) {
                slugErrors.add({
                  message: `Link to ${link} on page ${slug} does not exist`,
                  file: path,
                  line: lines.findIndex(line => line.includes(href)) + 1,
                });
              }
            }
          }
        }

        return '';
      },
    });
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
