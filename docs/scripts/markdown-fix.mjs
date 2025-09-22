#!/usr/bin/env node
import { readFileSync, writeFileSync } from 'fs';
import { unified } from 'unified';
import remarkParse from 'remark-parse';
import remarkStringify from 'remark-stringify';

/**
 * Plugin that:
 * 1) ## `Title`           -> ## Title
 * 2) ###### **Heading**   -> ###### <b>Heading</b>
 */
function pluginTransform() {
  return tree => {
    const visit = (node, fn, parent = null, index = -1) => {
      fn(node, parent, index);
      if (node.children) {
        node.children.forEach((child, i) => visit(child, fn, node, i));
      }
    };

    // Normalize lists to "tight" so Prettier won't remove blank lines later
    visit(tree, node => {
      if (node.type === 'list') {
        node.spread = false;
        if (Array.isArray(node.children)) {
          for (const li of node.children) {
            if (li && li.type === 'listItem') {
              li.spread = false;
            }
          }
        }
      }
    });

    visit(tree, node => {
      if (node.type !== 'heading') return;

      // Case 1: H2 with a single inlineCode -> replace with plain text
      if (
        node.depth === 2 &&
        node.children?.length === 1 &&
        node.children[0].type === 'inlineCode'
      ) {
        node.children = [{ type: 'text', value: node.children[0].value }];
      }

      // Case 2: H6 with a single strong -> wrap in HTML <b>...</b>
      if (
        node.depth === 6 &&
        node.children?.length === 1 &&
        node.children[0].type === 'strong'
      ) {
        const strong = node.children[0];
        const textOnly =
          strong.children?.length === 1 && strong.children[0].type === 'text'
            ? strong.children[0].value
            : null;

        if (textOnly !== null) {
          // Emit raw HTML inside the heading
          node.children = [{ type: 'html', value: `<b>${textOnly}</b>` }];
        }
      }
    });
  };
}

function transformMarkdown(input) {
  return unified()
    .use(remarkParse)
    .use(pluginTransform)
    .use(remarkStringify, {
      // Keep HTML the way we injected it
      handlers: {},
    })
    .processSync(input)
    .toString();
}

function main(argv) {
  const files = argv.slice(2);
  if (files.length === 0) {
    console.error('Usage: markdown-fix.mjs <file.md> [more.md]');
    process.exit(2);
  }

  for (const f of files) {
    const before = readFileSync(f, 'utf8');
    const after = transformMarkdown(before);
    if (after !== before) {
      writeFileSync(f, after, 'utf8');
      console.log(`Updated: ${f}`);
    } else {
      console.log(`OK (no change): ${f}`);
    }
  }
}

main(process.argv);
