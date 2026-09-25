import type { StackIdentity } from '../stack-adapter-contract.js';
import { hostedLease } from '../stack-lease-helpers.js';

export const MONGODB_ADAPTER_VERSION = '1.5.0';

export const MONGODB_IDENTITY: StackIdentity = {
  version: MONGODB_ADAPTER_VERSION,
  // Selects the `<!-- interface:<id> -->` blocks of the track contracts this stack sees.
  applicationInterface: 'http',
  ports: Object.freeze({ vite: 6423, express: 6101, db: 6537 }),
  lease: hostedLease('mongodb'),
};
