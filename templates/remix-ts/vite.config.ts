import { vitePlugin as remix } from '@remix-run/dev';
import { defineConfig } from 'vite';

// React Router GHSA-wrjc-x8rr-h8h6 / GHSA-jjmj-jmhj-qwj2 allow crafted link or
// redirect destinations to escape the origin or execute script. This template
// has no Link/useNavigate calls or redirects fed by user-supplied destinations.
// GHSA-337j-9hxr-rhxg can invoke unintended browser constructors when SSR error
// properties are attacker-controlled. Our loader catches fetch errors and returns
// an empty list; it never copies request data into serialized error properties.
export default defineConfig({
  plugins: [
    remix({
      future: {
        v3_fetcherPersist: true,
        v3_relativeSplatPath: true,
        v3_throwAbortReason: true,
        // GHSA-rxv8-25v2-qmq8: crafted loader data can exhaust CPU during
        // turbo-stream 2 single-fetch serialization. Keeping single-fetch off
        // prevents this Remix template from using that vulnerable transport.
        v3_singleFetch: false,
        v3_lazyRouteDiscovery: true,
      },
    }),
  ],
});
