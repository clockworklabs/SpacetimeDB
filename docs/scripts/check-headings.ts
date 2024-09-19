import { error } from 'console';
import { gatherData } from './gather-data';

export async function checkHeadingsOrder(
  data: Awaited<ReturnType<typeof gatherData>>
) {
  const errors: { file: string; line: number; message: string }[] = [];

  for (const [slug, { raw, path }] of Object.entries(data)) {
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
          errors.push({
            message: 'Heading level 1 is not allowed ',
            file: path,
            line: i,
          });
          continue;
        }

        const text = match[2];

        if (level >= currentNodes.length) {
          errors.push({
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
