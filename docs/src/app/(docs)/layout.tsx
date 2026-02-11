import { source } from '@/lib/source';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { TopNav } from '@/components/TopNav';
import type { ReactNode } from 'react';

export default function Layout({ children }: { children: ReactNode }) {
  return (
    <>
      <TopNav />
      <DocsLayout
        tree={source.getPageTree()}
        nav={{
          title: (
            // eslint-disable-next-line @next/next/no-img-element
            <img
              src="https://spacetimedb.com/images/brand.svg"
              alt="SpacetimeDB Logo"
              style={{ height: 32 }}
              className="md:hidden"
            />
          ),
          url: 'https://spacetimedb.com',
        }}
        searchToggle={{
          components: {
            lg: <></>,
          },
        }}
        sidebar={{
          defaultOpenLevel: 0,
          collapsible: false,
        }}
      >
        {children}
      </DocsLayout>
    </>
  );
}
