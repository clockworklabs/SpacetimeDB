import { join } from 'node:path';

import type { StackIdentity } from '../stack-adapter-contract.js';
import { leaseResources } from '../stack-lease-helpers.js';

export const SPACETIME_ADAPTER_VERSION = '1.4.0';

export const SPACETIME_IDENTITY: StackIdentity = {
  version: SPACETIME_ADAPTER_VERSION,
  // Selects the `<!-- interface:<id> -->` blocks of the track contracts this stack sees.
  applicationInterface: 'reducer',
  ports: Object.freeze({ vite: 6173 }),
  lease: {
    prepare(input) {
      return {
        lease: {
          serverUri: input.serverUri,
          database: null,
          module: input.helpers.moduleName(input.track, input.runIndex),
          dataDir: join(input.runtimeDir, 'spacetime-data'),
          container: null,
        },
        lockKeys: [`listener:${input.serverUri}`],
      };
    },
    validateResources(input) {
      const value = leaseResources(input);
      input.helpers.loopbackHttpUri(value.serverUri);
      input.helpers.requireString(value.module, 'module');
      input.helpers.requireString(value.dataDir, 'dataDir');
      if (!Array.isArray(value.listenerProcesses)) {
        throw new Error('listenerProcesses must be an array');
      }
    },
  },
};
