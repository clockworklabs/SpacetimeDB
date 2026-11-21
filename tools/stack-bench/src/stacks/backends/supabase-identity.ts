import type { StackIdentity } from '../stack-adapter-contract.js';
import { ownedPlatformLease } from '../stack-lease-helpers.js';

export const SUPABASE_ADAPTER_VERSION = '1.0.0';
export const DEFAULT_SUPABASE_GATEWAY_URI = 'http://127.0.0.1:13410';

export const SUPABASE_IMAGES = Object.freeze({
  db: 'supabase/postgres@sha256:f371b5f3f2ac0a05703f33d6e6134515fb2498cab708fb948a0aeb7481467c00',
  auth: 'supabase/gotrue@sha256:1736a63078f5922b198c4cbe50f80ab9a2d3b54fe8b7b6cfb2e9dc5dbbc12c6b',
  rest: 'postgrest/postgrest@sha256:c7cd7e265a85b05248abf53f4473fcec7e327c282d0bd0cf60f1551996d09fcf',
  realtime: 'supabase/realtime@sha256:25cf375b9b7cb71d90cdf6e04a1a30996e39a8df866ebd0525bba1d959edc8d6',
  storage: 'supabase/storage-api@sha256:8526956cb462e9f7d392e9848ec77133b2769982ed614e9753cc687e8e4339a6',
  functions: 'supabase/edge-runtime@sha256:6b850a2ccf0bf50ecac9accff8098095cd191c389c2a53c59b857aa0f53a8343',
  gateway: 'envoyproxy/envoy@sha256:1fde8efee875e4e4f6a39a90fe49d9338e1caacce5762034ef9fb4bbb9ad6aed',
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
