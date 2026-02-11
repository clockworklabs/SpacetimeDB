'use client';

import type { SharedProps } from 'fumadocs-ui/components/dialog/search';
import { InkeepModalSearchAndChat } from '@inkeep/cxkit-react';

export default function InkeepSearch(props: SharedProps) {
  return (
    <InkeepModalSearchAndChat
      baseSettings={{
        apiKey: '13504c49fb56b7c09a5ea0bcd68c2b55857661be4d6d311b',
        organizationDisplayName: 'SpacetimeDB',
        primaryBrandColor: '#4cf490',
        colorMode: {
          forcedColorMode: 'dark',
        },
      }}
      modalSettings={{
        isOpen: props.open,
        onOpenChange: props.onOpenChange,
      }}
    />
  );
}
