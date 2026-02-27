import { useMemo } from 'react';
import { useDocsVersion } from '@docusaurus/plugin-content-docs/client';
import { CardLinkGrid } from './CardLinkGrid';
import type { Item } from './CardLink';
import ReactLogo from '@site/static/images/logos/react-logo.svg';
import NextJSLogo from '@site/static/images/logos/nextjs-logo.svg';
import VueLogo from '@site/static/images/logos/vue-logo.svg';
import NuxtLogo from '@site/static/images/logos/nuxt-logo.svg';
import SvelteLogo from '@site/static/images/logos/svelte-logo.svg';
import AngularLogo from '@site/static/images/logos/angular-logo.svg';
import TanStackLogo from '@site/static/images/logos/tanstack-logo.svg';
import RemixLogo from '@site/static/images/logos/remix-logo.svg';
import Html5Logo from '@site/static/images/logos/html5-logo.svg';
import BunLogo from '@site/static/images/logos/bun-logo.svg';
import DenoLogo from '@site/static/images/logos/deno-logo.svg';
import NodeJSLogo from '@site/static/images/logos/nodejs-logo.svg';
import TypeScriptLogo from '@site/static/images/logos/typescript-logo.svg';
import RustLogo from '@site/static/images/logos/rust-logo.svg';
import CSharpLogo from '@site/static/images/logos/csharp-logo.svg';
import CppLogo from '@site/static/images/logos/cpp-logo.svg';

const ALL_ITEMS: Item[] = [
  {
    icon: <ReactLogo height={40} />,
    href: 'quickstarts/react',
    docId: 'intro/quickstarts/react',
    label: 'React',
  },
  {
    icon: <NextJSLogo height={40} />,
    href: 'quickstarts/nextjs',
    docId: 'intro/quickstarts/nextjs',
    label: 'Next.js',
  },
  {
    icon: <VueLogo height={40} />,
    href: 'quickstarts/vue',
    docId: 'intro/quickstarts/vue',
    label: 'Vue',
  },
  {
    icon: <NuxtLogo height={40} />,
    href: 'quickstarts/nuxt',
    docId: 'intro/quickstarts/nuxt',
    label: 'Nuxt',
  },
  {
    icon: <SvelteLogo height={40} />,
    href: 'quickstarts/svelte',
    docId: 'intro/quickstarts/svelte',
    label: 'Svelte',
  },
  {
    icon: <AngularLogo height={40} />,
    href: 'quickstarts/angular',
    docId: 'intro/quickstarts/angular',
    label: 'Angular',
  },
  {
    icon: <TanStackLogo height={40} />,
    href: 'quickstarts/tanstack',
    docId: 'intro/quickstarts/tanstack',
    label: 'TanStack Start',
  },
  {
    icon: <RemixLogo height={40} />,
    href: 'quickstarts/remix',
    docId: 'intro/quickstarts/remix',
    label: 'Remix',
  },
  {
    icon: <Html5Logo height={40} />,
    href: 'quickstarts/browser',
    docId: 'intro/quickstarts/browser',
    label: 'Browser',
  },
  {
    icon: <BunLogo height={40} />,
    href: 'quickstarts/bun',
    docId: 'intro/quickstarts/bun',
    label: 'Bun',
  },
  {
    icon: <DenoLogo height={40} />,
    href: 'quickstarts/deno',
    docId: 'intro/quickstarts/deno',
    label: 'Deno',
  },
  {
    icon: <NodeJSLogo height={40} />,
    href: 'quickstarts/nodejs',
    docId: 'intro/quickstarts/nodejs',
    label: 'Node.js',
  },
  {
    icon: <TypeScriptLogo height={40} />,
    href: 'quickstarts/typescript',
    docId: 'intro/quickstarts/typescript',
    label: 'TypeScript',
  },
  {
    icon: <RustLogo height={40} />,
    href: 'quickstarts/rust',
    docId: 'intro/quickstarts/rust',
    label: 'Rust',
  },
  {
    icon: <CSharpLogo height={40} />,
    href: 'quickstarts/c-sharp',
    docId: 'intro/quickstarts/c-sharp',
    label: 'C#',
  },
  {
    icon: <CppLogo height={40} />,
    href: 'quickstarts/c-plus-plus',
    docId: 'intro/quickstarts/cpp',
    label: 'C++',
  },
];

export function QuickstartLinks() {
  const version = useDocsVersion();
  const items = useMemo(() => {
    const docIds = new Set(Object.keys(version.docs));
    return ALL_ITEMS.filter((item) => !item.docId || docIds.has(item.docId));
  }, [version]);

  return <CardLinkGrid items={items} />;
}
