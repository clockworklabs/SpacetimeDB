import type { StackIdentity } from '../stack-adapter-contract.js';
import { hostedLease } from '../stack-lease-helpers.js';

export const POSTGRES_ADAPTER_VERSION = '1.6.0';

export const POSTGRES_IDENTITY: StackIdentity = {
  version: POSTGRES_ADAPTER_VERSION,
  // Selects the `<!-- interface:<id> -->` blocks of the track contracts this stack sees.
  applicationInterface: 'http',
  ports: Object.freeze({ vite: 6273, express: 6001, db: 6532 }),
  lease: hostedLease('postgres'),
};
