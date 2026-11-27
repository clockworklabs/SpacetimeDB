import type { StackDatabaseRuntime } from '../process-crash.js';
import { loadTrack, portsFor } from '../../composition/tracks.js';
import { answers } from '../lifecycle-readiness.js';
import { CONVEX_PROCESS_RECORD, recoverConvex } from './convex-lifecycle.js';

// Convex functions run inside the backend process.
export const CONVEX_RUNTIME: StackDatabaseRuntime = {
  combinedBoundary: true,
  databaseUser: '0:0',
  processRecord: CONVEX_PROCESS_RECORD,
  recoverDatabase: ({ leasePath, lease, signal }) => recoverConvex({ leasePath, leaseToken: lease.ownershipToken,
    ports: portsFor(loadTrack(lease.track), 'convex', lease.runIndex), signal }),
  databaseReady: lease => answers(`${lease.resources.serverUri}/version`, { requireSuccess: true }),
};
