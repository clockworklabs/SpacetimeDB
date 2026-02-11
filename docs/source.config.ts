import { defineConfig, defineDocs } from 'fumadocs-mdx/config';
import remarkDirective from 'remark-directive';
import { remarkDirectiveAdmonition } from 'fumadocs-core/mdx-plugins';

export const docs = defineDocs({
  dir: 'content/docs',
});

export default defineConfig({
  mdxOptions: {
    remarkPlugins: [
      remarkDirective,
      [remarkDirectiveAdmonition, {
        types: {
          note: 'info',
          tip: 'info',
          info: 'info',
          warn: 'warning',
          warning: 'warning',
          danger: 'error',
          caution: 'warning',
          success: 'success',
        },
      }],
    ],
    rehypeCodeOptions: {
      themes: {
        light: 'dracula',
        dark: 'dracula',
      },
      langs: [
        'sql', 'rust', 'csharp', 'typescript', 'bash', 'json', 'toml',
        'python', 'c', 'cpp', 'proto', 'fsharp', 'systemd', 'tsx',
        'css', 'nginx', 'markdown', 'xml', 'yaml', 'powershell', 'shellscript',
      ],
      defaultLanguage: 'text',
    },
  },
});
