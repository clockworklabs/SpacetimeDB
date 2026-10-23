import { vitePlugin as remix } from '@remix-run/dev';
import { defineConfig } from 'vite';

// This template has no user-controlled Link/redirect destinations and does not
// copy request data into error properties used by SSR hydration. Reassess the
// React Router advisories if either behavior is added.
export default defineConfig({
  plugins: [
    remix({
      future: {
        v3_fetcherPersist: true,
        v3_relativeSplatPath: true,
        v3_throwAbortReason: true,
        // Keep the multi-request transport; the single-fetch path uses turbo-stream 2.
        v3_singleFetch: false,
        v3_lazyRouteDiscovery: true,
      },
    }),
  ],
});
