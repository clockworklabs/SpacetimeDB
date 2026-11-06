import type { StackDatabaseRuntime } from '../process-crash.js';
import { attemptDocker } from '../../runtime/docker-network.js';
import { attemptDatabaseIdentity } from '../hosted-database-identity.js';
import { startAttemptDatabaseProcess } from '../hosted-lifecycle.js';

export const MONGODB_RUNTIME: StackDatabaseRuntime = {
  combinedBoundary: false,
  databaseUser: 'mongodb',
  processRecord: null,
  recoverDatabase: ({ lease }) => startAttemptDatabaseProcess(lease),
  async databaseReady(lease) {
    try {
      attemptDocker(['exec', lease.resources.container!.id, 'mongosh', '--quiet', '--username', 'admin', '--password',
        attemptDatabaseIdentity(lease.ownershipToken).adminPassword, '--authenticationDatabase', 'admin',
        '--eval', 'if (!db.hello().isWritablePrimary) quit(1)']);
      return true;
    } catch { return false; }
  },
  drainCommand(lease, database) {
    const identity = attemptDatabaseIdentity(lease.ownershipToken);
    // A single pooled connection lets us exclude precisely this observer, not
    // app transactions that no longer have a live connection.
    return ['mongosh', `mongodb://127.0.0.1/${encodeURIComponent(database)}?maxPoolSize=1&directConnection=true`,
      '--username', identity.user, '--password', identity.password, '--authenticationDatabase', database,
      '--quiet', '--eval', `
        const self = db.hello().connectionId;
        if (!Number.isSafeInteger(Number(self)) || Number(self) <= 0) throw new Error('missing observer connection');
        const admin = db.getSiblingDB('admin');
        const work = admin.aggregate([
          {$currentOp:{allUsers:false,idleConnections:true,idleSessions:true}},
          {$match:{connectionId:{$ne:self}}}
        ]).toArray();
        // The killed app cannot resume its idle transactions. End them now instead
        // of waiting up to 90 seconds for MongoDB's 60-second transaction sweep.
        const idle = work.filter(op => op.type === 'idleSession' && op.lsid).map(op => ({ id: op.lsid.id }));
        if (idle.length && admin.runCommand({ killSessions: idle }).ok !== 1) throw new Error('could not end orphaned sessions');
        print(work.length);`];
  },
};
