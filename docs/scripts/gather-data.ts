import { readdir, readFile } from 'node:fs/promises';

export async function gatherData() {
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

const removeQuotes = (str: string) => str.replace(/(^["']|["']$)/g, '');
