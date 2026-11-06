import type { StackDatabaseRuntime } from '../process-crash.js';
import { SPACETIME_PROCESS_RECORD, startAttemptDatabaseProcess } from '../hosted-lifecycle.js';
import { answers } from '../lifecycle-readiness.js';

// The module runs inside the database process.
export const SPACETIME_RUNTIME: StackDatabaseRuntime = {
  combinedBoundary: true,
  databaseUser: '0:0',
  processRecord: SPACETIME_PROCESS_RECORD,
  recoverDatabase: ({ lease }) => startAttemptDatabaseProcess(lease),
  databaseReady: lease => answers(`${lease.resources.serverUri}/v1/ping`, { requireSuccess: true }),
};
