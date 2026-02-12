import { source, getLLMText } from '@/lib/source';
import type { InferPageType } from 'fumadocs-core/source';
import {
  DocsBody,
  DocsDescription,
  DocsPage,
  DocsTitle,
  EditOnGitHub,
} from 'fumadocs-ui/layouts/docs/page';
import { notFound } from 'next/navigation';
import { getMDXComponents } from '../../../../mdx-components';
import mdxComponents, { createRelativeLink } from 'fumadocs-ui/mdx';
import type { Metadata } from 'next';
import type { ComponentProps } from 'react';
import { CopyPageButton } from '@/components/copy-page-button';

type Page = InferPageType<typeof source>;

function normalizeLegacyDocsHref(href: string): string {
  if (!href.startsWith('/docs')) return href;

  const nextChar = href.charAt('/docs'.length);
  if (nextChar && nextChar !== '/' && nextChar !== '?' && nextChar !== '#') {
    return href;
  }

  const stripped = href.slice('/docs'.length);
  if (stripped.length === 0) return '/';
  return stripped.startsWith('/') ? stripped : `/${stripped}`;
}

export default async function Page(props: {
  params: Promise<{ slug?: string[] }>;
}) {
  const params = await props.params;
  const page = source.getPage(params.slug) as Page | undefined;
  if (!page) notFound();

  const MDX = page.data.body;
  const llmText = await getLLMText(page);
  const RelativeLink = createRelativeLink(
    source,
    page,
    function LegacyDocsLink({ href, ...props }: ComponentProps<'a'>) {
      return (
        <mdxComponents.a
          href={typeof href === 'string' ? normalizeLegacyDocsHref(href) : href}
          {...props}
        />
      );
    },
  );

  return (
    <DocsPage
      toc={page.data.toc}
      full={page.data.full}
      breadcrumb={{ includeRoot: true }}
      footer={{
        children: (
          <EditOnGitHub
            href={`https://github.com/clockworklabs/SpacetimeDB/edit/master/docs/content/docs/${page.path}`}
          />
        ),
      }}
    >
      <div className="relative">
        <DocsTitle>{page.data.title}</DocsTitle>
        <DocsDescription>{page.data.description}</DocsDescription>
        <div className="absolute right-0 top-0">
          <CopyPageButton content={llmText} />
        </div>
      </div>
      <DocsBody>
        <MDX
          components={getMDXComponents({
            a: RelativeLink,
          })}
        />
      </DocsBody>
    </DocsPage>
  );
}

export async function generateStaticParams() {
  return source.generateParams();
}

export async function generateMetadata(props: {
  params: Promise<{ slug?: string[] }>;
}): Promise<Metadata> {
  const params = await props.params;
  const page = source.getPage(params.slug);
  if (!page) notFound();

  return {
    title: page.data.title,
    description: page.data.description,
  };
}
