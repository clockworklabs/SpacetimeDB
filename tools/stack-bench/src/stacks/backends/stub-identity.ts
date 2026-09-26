import type { StackIdentity } from '../stack-adapter-contract.js';

export const STUB_ADAPTER_VERSION = '1.1.0';

export const STUB_IDENTITY: StackIdentity = {
  version: STUB_ADAPTER_VERSION,
  // Selects the `<!-- interface:<id> -->` blocks of the track contracts this stack sees.
  applicationInterface: 'http',
  ports: Object.freeze({ vite: 7000 }),
  lease: {
    prepare: () => ({
      lease: { serverUri: null, database: null, module: null, dataDir: null, container: null },
      lockKeys: [],
    }),
    validateResources: () => undefined,
  },
};
