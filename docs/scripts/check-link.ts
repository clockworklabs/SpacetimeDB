import { gatherData } from './gather-data';
import { Marked, Renderer, TokenizerObject } from 'marked';

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

export async function transform(
  markdown: string,
  renderer: Partial<Renderer> = {}
) {
  const marked = new Marked({
    renderer,
    tokenizer,
  });

  return await marked.parse(markdown);
}

export async function checkLinks(data: Awaited<ReturnType<typeof gatherData>>) {
  const errors: { file: string; line: number; message: string }[] = [];

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
  for (const [slug, { raw, path, content }] of Object.entries(data)) {
    const lines = raw.split('\n');

    await transform(content, {
      link({ href }) {
        if (href.startsWith('#')) {
          const link = href.slice(1);
          if (!headingsOnPages.get(slug)!.has(link)) {
            errors.push({
              message: `Link to ${link} on page ${slug} does not exist`,
              file: path,
              line: lines.findIndex(line => line.includes(href)),
            });
          }
        } else {
          //  Should start with /docs. Then compare, including any hash it might have. Examples: /docs/data-format/bsatn or /docs/introduction/getting-started#some-heading
          const link = href.slice(1);
          if (link.startsWith('docs/')) {
            const slug = link.slice(4);
            const hashIfThere = slug.includes('#')
              ? slug.slice(slug.indexOf('#'))
              : null;

            if (!headingsOnPages.has(slug)) {
              errors.push({
                message: `Link to ${link} on page ${slug} does not exist`,
                file: path,
                line: lines.findIndex(line => line.includes(href)),
              });
            } else {
              if (hashIfThere) {
                if (!headingsOnPages.get(slug)!.has(hashIfThere)) {
                  errors.push({
                    message: `Link to ${link} on page ${slug} does not exist`,
                    file: path,
                    line: lines.findIndex(line => line.includes(href)),
                  });
                }
              }
            }
          }
        }

        return '';
      },
    });
  }

  if (errors.length > 0) {
    console.log('ERRORS:');
    for (const error of errors) {
      console.log(
        error.message,
        ':',
        new URL(`../content/docs/${error.file}`, import.meta.url).href +
          '#' +
          (error.line + 1)
      );
    }
  }
}

export function slugify(title: string) {
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

export function removeMarkdown(markdown: string) {
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

export function removeHTMLEntities(html: string) {
  return html.replace(/&.+?;/g, '');
}

export const normalizeSlugify = (str: string) => {
  return slugify(removeHTMLEntities(removeMarkdown(str))).replace(
    /(<([^>]+)>)/gi,
    ''
  );
};
