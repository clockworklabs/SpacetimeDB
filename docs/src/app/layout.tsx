import '@fontsource-variable/inter';
import '@fontsource-variable/source-code-pro';
import './global.css';
import { Provider } from '@/components/Provider';
import { AskAiButton } from '@/components/AskAiButton';
import type { ReactNode } from 'react';

export const metadata = {
  title: 'SpacetimeDB docs',
  description: 'SpacetimeDB documentation',
  icons: {
    icon: 'https://spacetimedb.com/favicon-32x32.png',
  },
};

export default function RootLayout({ children }: { children: ReactNode }) {
  return (
    <html lang="en" className="dark" suppressHydrationWarning>
      <head>
        <link
          rel="preload"
          as="font"
          type="font/woff2"
          href="/docs/fonts/inter-latin-wght-normal.woff2"
          crossOrigin="anonymous"
        />
        <link
          rel="preload"
          as="font"
          type="font/woff2"
          href="/docs/fonts/source-code-pro-latin-wght-normal.woff2"
          crossOrigin="anonymous"
        />
      </head>
      <body>
        <Provider>
          {children}
          <AskAiButton />
        </Provider>
      </body>
    </html>
  );
}
