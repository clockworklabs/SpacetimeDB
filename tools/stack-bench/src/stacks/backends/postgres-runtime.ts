import type { StackDatabaseRuntime } from '../process-crash.js';
import { attemptDocker } from '../../runtime/docker-network.js';
import { attemptDatabaseIdentity } from '../hosted-database-identity.js';
import { startAttemptDatabaseProcess } from '../hosted-lifecycle.js';

export const POSTGRES_RUNTIME: StackDatabaseRuntime = {
  combinedBoundary: false,
  databaseUser: 'postgres',
  processRecord: null,
  recoverDatabase: ({ lease }) => startAttemptDatabaseProcess(lease),
  async databaseReady(lease) {
    try {
      attemptDocker(['exec', lease.resources.container!.id, 'pg_isready', '-h', '127.0.0.1', '-U', 'postgres']);
      return true;
    } catch { return false; }
  },
  drainCommand(lease, database) {
    const identity = attemptDatabaseIdentity(lease.ownershipToken);
    return ['psql', '-U', identity.user, '-d', database, '-v', 'ON_ERROR_STOP=1', '-At', '-c',
      "SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND backend_type='client backend' AND pid<>pg_backend_pid()"];
  },
};
