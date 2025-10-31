import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';
import rehypeShiki, { RehypeShikiOptions } from '@shikijs/rehype';
import bash from 'shiki/langs/bash.mjs';
import c from 'shiki/langs/c.mjs';
import csharp from 'shiki/langs/csharp.mjs';
import fsharp from 'shiki/langs/fsharp.mjs';
import json from 'shiki/langs/json.mjs';
import markdown from 'shiki/langs/markdown.mjs';
import protobuf from 'shiki/langs/proto.mjs';
import python from 'shiki/langs/python.mjs';
import rust from 'shiki/langs/rust.mjs';
import sql from 'shiki/langs/sql.mjs';
import toml from 'shiki/langs/toml.mjs';
import typescript from 'shiki/langs/typescript.mjs';
import tsx from 'shiki/langs/tsx.mjs';
import css from 'shiki/langs/css.mjs';
import nginx from 'shiki/langs/nginx.mjs';
import systemd from 'shiki/langs/systemd.mjs';
import ogTheme from 'shiki/themes/dracula.mjs';
import cpp from 'shiki/langs/cpp.mjs';
import { InkeepConfig } from '@inkeep/cxkit-docusaurus';

// This runs in Node.js - Don't use client-side code here (browser APIs, JSX...)

const shikiTheme = {
  ...ogTheme,
  name: 'spacetime-dark',
};
shikiTheme.colors!['editor.background'] =
  'var(--clockworklabs-code-background-color)';

const inkeepConfig: Partial<InkeepConfig> = {
  baseSettings: {
    apiKey: 'fe642f176d62fd867d93c0934f3a7c2336097bcef5f43e92',
    organizationDisplayName: 'SpacetimeDB',
    primaryBrandColor: '#4cf490',
    colorMode: {
      forcedColorMode: 'dark',
    },
  },
};

const config: Config = {
  title: 'SpacetimeDB docs',
  tagline: 'SpacetimeDB',
  favicon: 'https://spacetimedb.com/favicon-32x32.png',

  url: 'https://spacetimedb.com',
  baseUrl: '/docs/',

  onBrokenLinks: 'throw',
  onBrokenAnchors: 'throw',
  markdown: {
    hooks: {
      onBrokenMarkdownImages: 'throw',
      onBrokenMarkdownLinks: 'throw',
    },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  clientModules: [require.resolve('./src/client-modules/fonts')],

  headTags: [
    {
      tagName: 'link',
      attributes: {
        rel: 'preload',
        as: 'font',
        type: 'font/woff2',
        href: '/docs/fonts/inter-latin-wght-normal.woff2',
        crossorigin: 'anonymous',
      },
    },
    {
      tagName: 'link',
      attributes: {
        rel: 'preload',
        as: 'font',
        type: 'font/woff2',
        href: '/docs/fonts/source-code-pro-latin-wght-normal.woff2',
        crossorigin: 'anonymous',
      },
    },
  ],

  presets: [
    [
      'classic',
      {
        docs: {
          editUrl: "https://github.com/clockworklabs/SpacetimeDB/edit/master/docs/",
          routeBasePath: '/',
          sidebarPath: './sidebars.ts',
          sidebarCollapsed: false,
          beforeDefaultRehypePlugins: [
            [
              rehypeShiki,
              {
                theme: shikiTheme,
                langs: [
                  sql,
                  rust,
                  csharp,
                  markdown,
                  typescript,
                  bash,
                  json,
                  toml,
                  python,
                  c,
                  cpp,
                  protobuf,
                  fsharp,
                  systemd,
                  tsx,
                  css,
                  nginx,
                ],
              } satisfies RehypeShikiOptions,
            ],
          ],
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    navbar: {
      logo: {
        alt: 'SpacetimeDB Logo',
        src: 'https://spacetimedb.com/images/brand.png',
        href: 'https://spacetimedb.com',
        target: '_self',
      },
      hideOnScroll: false,
      items: [
        { type: 'search', position: 'left' },
        {
          href: 'https://spacetimedb.com/install',
          label: 'Install',
          position: 'right',
        },
        {
          href: 'https://spacetimedb.com/pricing',
          label: 'Pricing',
          position: 'right',
        },
        {
          href: 'https://spacetimedb.com/maincloud',
          label: 'Maincloud',
          position: 'right',
        },
        {
          href: 'https://spacetimedb.com/blog',
          label: 'Blog',
          position: 'right',
        },
        {
          href: 'https://spacetimedb.com/community',
          label: 'Community',
          position: 'right',
        },
        {
          href: 'https://spacetimedb.com/login',
          label: 'Login',
          position: 'right',
          className: 'navbar__button',
        },
      ],
    },
    footer: {},
    prism: {},
    colorMode: {
      disableSwitch: true,
      defaultMode: 'light',
    },
  } satisfies Preset.ThemeConfig,

  plugins: [
    [
      '@inkeep/cxkit-docusaurus',
      {
        SearchBar: {
          ...inkeepConfig,
        },
        ChatButton: {
          ...inkeepConfig,
        },
      },
    ],
  ],
};

export default config;
