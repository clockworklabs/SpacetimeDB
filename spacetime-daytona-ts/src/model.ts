import { table, t, type Infer } from 'spacetimedb/server';

export const sandbox = table(
  { name: 'sandbox' },
  {
    id: t.u64().primaryKey().autoInc(),
    owner: t.string().index('btree'),
    request: t.string().unique(),
    remoteName: t.string(),
    snapshot: t.string(),
    ttlMinutes: t.u32(),
    remoteId: t.string().optional(),
    state: t.enum('DaytonaSandboxState', [
      'Queued',
      'Creating',
      'Ready',
      'Unavailable',
      'Unknown',
      'Deleting',
      'Deleted',
    ]),
    deleteRequested: t.bool(),
    expiresAt: t.timestamp().optional(),
    claim: t.u64(),
    checkAt: t.timestamp().index('btree'),
    error: t.string().optional(),
    createdAt: t.timestamp(),
  }
);

export const execution = table(
  { name: 'execution' },
  {
    id: t.u64().primaryKey().autoInc(),
    sandboxId: t.u64().index('btree'),
    request: t.string().unique(),
    command: t.string(),
    sessionId: t.string(),
    commandId: t.string().optional(),
    state: t.enum('DaytonaExecutionState', [
      'Queued',
      'Submitting',
      'Running',
      'Succeeded',
      'Failed',
      'Unknown',
    ]),
    exitCode: t.i32().optional(),
    claim: t.u64(),
    checkAt: t.timestamp().index('btree'),
    error: t.string().optional(),
    finishedAt: t.timestamp().optional(),
  }
);

// A host procedure can only schedule a table registered in its own schema.
export const daytonaTick = table(
  { name: 'daytona_tick' },
  {
    scheduledId: t.u64().primaryKey(),
    scheduledAt: t.scheduleAt(),
  }
);

export type Sandbox = Infer<typeof sandbox.rowType>;
export type Execution = Infer<typeof execution.rowType>;
