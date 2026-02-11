'use client';

import { RootProvider } from 'fumadocs-ui/provider/next';
import dynamic from 'next/dynamic';
import type { ReactNode } from 'react';

const InkeepSearch = dynamic(() => import('@/components/InkeepSearch'));

export function Provider({ children }: { children: ReactNode }) {
  return (
    <RootProvider
      theme={{
        enabled: false,
      }}
      search={{
        SearchDialog: InkeepSearch,
      }}
    >
      {children}
    </RootProvider>
  );
}
