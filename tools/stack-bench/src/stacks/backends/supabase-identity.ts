import type { StackIdentity } from '../stack-adapter-contract.js';
import { ownedPlatformLease } from '../stack-lease-helpers.js';

export const SUPABASE_ADAPTER_VERSION = '1.0.0';
export const DEFAULT_SUPABASE_GATEWAY_URI = 'http://127.0.0.1:13410';

export const SUPABASE_IMAGES = Object.freeze({
  db: 'supabase/postgres@sha256:f371b5f3f2ac0a05703f33d6e6134515fb2498cab708fb948a0aeb7481467c00',
  auth: 'supabase/gotrue@sha256:c0c25187a6b835e65a6f6e6c6b39d090e832d40e6de5186f2c038e0411944232',
  rest: 'postgrest/postgrest@sha256:c9dc201e555f5d8e37e7f39cdd4df0229774996e213bfd7de8d10ac609030f2c',
  realtime: 'supabase/realtime@sha256:cbcc6a7986fc28b6dcffa798b077d5fb9c69cd25500371ab49147a86d7edbb03',
  storage: 'supabase/storage-api@sha256:f1546fac6d1c7e345428ac904bfaa7be7cecd50a1f549fe1cf38c628a7b15c85',
  functions: 'supabase/edge-runtime@sha256:edd22bef4477b900d5c300e287ce9b18bff9b81a0291bee14ee0b7c7b71a2899',
  gateway: 'envoyproxy/envoy@sha256:57e14a549d7bd43c8d3f6d03e8cfa653e037d4b38e133acd9b54f38c524401b4',
});

export const SUPABASE_IDENTITY: StackIdentity = {
  version: SUPABASE_ADAPTER_VERSION,
  // Selects the `<!-- interface:<id> -->` blocks of the track contracts this stack sees.
  applicationInterface: 'supabase',
  // `express` is the optional application server; the platform gateway has its own port.
  ports: Object.freeze({ vite: 7823, express: 7901 }),
  // serverUri is the platform gateway; the anchor container runs the database.
  lease: ownedPlatformLease({ label: 'Supabase', database: 'postgres' }),
  // Held for an attempt's first minute until qualification measures the platform.
  startupMemoryBytes: 3 * 1024 ** 3,
  releaseImages: Object.freeze(Object.entries(SUPABASE_IMAGES).map(([role, reference]) =>
    ({ role: `supabase-${role}`, reference, description: `Supabase ${role} image` }))),
};
