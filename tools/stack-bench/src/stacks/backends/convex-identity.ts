import type { StackIdentity } from '../stack-adapter-contract.js';
import { ownedPlatformLease } from '../stack-lease-helpers.js';

export const CONVEX_ADAPTER_VERSION = '1.0.0';
export const DEFAULT_CONVEX_SERVER_URI = 'http://127.0.0.1:13210';
export const CONVEX_BACKEND_IMAGE = 'ghcr.io/convex/convex-backend@sha256:7427e3eef2b79cac55e2488ad0ef235baf53bbf7c18902f6fb5e02d2d049f88a';

export const CONVEX_IDENTITY: StackIdentity = {
  version: CONVEX_ADAPTER_VERSION,
  // Selects the `<!-- interface:<id> -->` blocks of the track contracts this stack sees.
  applicationInterface: 'convex',
  ports: Object.freeze({ vite: 6623, express: 6701 }),
  lease: ownedPlatformLease({ label: 'Convex' }),
  releaseImages: Object.freeze([
    { role: 'convex', reference: CONVEX_BACKEND_IMAGE, description: 'Convex backend image' },
  ]),
};
