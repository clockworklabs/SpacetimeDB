import { CardLinkGrid } from './CardLinkGrid';
import { Item } from './CardLink';

const items: Item[] = [
  {
    href: '/quickstarts/typescript',
    label: 'TypeScript',
    description: 'Get a SpacetimeDB TypeScript app running in under 5 minutes.',
    icon: (
      <img
        src="/docs/images/logos/typescript-logo.svg"
        width={40}
        height={40}
        alt="TypeScript"
        style={{ width: 40, height: 40 }}
      />
    ),
  },
  {
    href: '/quickstarts/c-sharp',
    label: 'C#',
    description: 'Get a SpacetimeDB C# app running in under 5 minutes.',
    icon: (
      <img
        src="/docs/images/logos/csharp-logo.svg"
        width={40}
        height={40}
        alt="C#"
        style={{ width: 40, height: 40 }}
      />
    ),
  },
  {
    href: '/quickstarts/rust',
    label: 'Rust',
    description: 'Get a SpacetimeDB Rust app running in under 5 minutes.',
    icon: (
      <img
        src="/docs/images/logos/rust-logo.svg"
        width={40}
        height={40}
        alt="Rust"
        style={{ width: 40, height: 40 }}
      />
    ),
  },
  {
    href: '/quickstarts/react',
    label: 'React',
    description: 'Get a SpacetimeDB React app running in under 5 minutes.',
    icon: (
      <img
        src="/docs/images/logos/react-logo.svg"
        width={40}
        height={40}
        alt="React"
        style={{ width: 40, height: 40 }}
      />
    ),
  },
];

export function QuickstartLinks() {
  return <CardLinkGrid items={items} />;
}
