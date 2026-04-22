import React from 'react';
import Link from '@docusaurus/Link';
import { useDocsVersion } from '@docusaurus/plugin-content-docs/client';
import DocVersionBanner from '@theme-original/DocVersionBanner';

export default function DocVersionBannerWrapper(
  props: React.ComponentProps<typeof DocVersionBanner>,
): JSX.Element {
  const version = useDocsVersion();

  if (version.version === '1.12.0') {
    return (
      <div className="alert alert--info margin-bottom--md" role="alert">
        Looking for the latest features? Try the <Link to="/">2.0.0 docs</Link>.
      </div>
    );
  }

  return <DocVersionBanner {...props} />;
}
