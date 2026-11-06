import { table, t, type Infer, type SumBuilder } from 'spacetimedb/server';

type DaytonaSandboxStateBuilder = SumBuilder<
  Record<
    | 'Queued'
    | 'Creating'
    | 'Ready'
    | 'Unavailable'
    | 'Unknown'
    | 'Deleting'
    | 'Deleted',
    ReturnType<typeof t.unit>
  >
>;
const sandboxState: DaytonaSandboxStateBuilder = t.enum('DaytonaSandboxState', {
  Queued: t.unit(),
  Creating: t.unit(),
  Ready: t.unit(),
  Unavailable: t.unit(),
  Unknown: t.unit(),
  Deleting: t.unit(),
  Deleted: t.unit(),
});

type DaytonaExecutionStateBuilder = SumBuilder<
  Record<
    'Queued' | 'Submitting' | 'Running' | 'Succeeded' | 'Failed' | 'Unknown',
    ReturnType<typeof t.unit>
  >
>;
const executionState: DaytonaExecutionStateBuilder = t.enum(
  'DaytonaExecutionState',
  {
    Queued: t.unit(),
    Submitting: t.unit(),
    Running: t.unit(),
    Succeeded: t.unit(),
    Failed: t.unit(),
    Unknown: t.unit(),
  }
);

const sandboxOptions = { name: 'sandbox' };
const sandboxColumns = {
  id: t.u64().primaryKey().autoInc(),
  owner: t.string().index('btree'),
  request: t.string().unique(),
  remoteName: t.string(),
  snapshot: t.string(),
  ttlMinutes: t.u32(),
  remoteId: t.string().optional(),
  state: sandboxState,
  deleteRequested: t.bool(),
  expiresAt: t.timestamp().optional(),
  claim: t.u64(),
  checkAt: t.timestamp().index('btree'),
  error: t.string().optional(),
  createdAt: t.timestamp(),
};
export const sandbox: ReturnType<
  typeof table<typeof sandboxColumns, typeof sandboxOptions>
> = table(sandboxOptions, sandboxColumns);

const executionOptions = { name: 'execution' };
const executionColumns = {
  id: t.u64().primaryKey().autoInc(),
  sandboxId: t.u64().index('btree'),
  request: t.string().unique(),
  command: t.string(),
  sessionId: t.string(),
  commandId: t.string().optional(),
  state: executionState,
  exitCode: t.i32().optional(),
  claim: t.u64(),
  checkAt: t.timestamp().index('btree'),
  error: t.string().optional(),
  finishedAt: t.timestamp().optional(),
};
export const execution: ReturnType<
  typeof table<typeof executionColumns, typeof executionOptions>
> = table(executionOptions, executionColumns);

const daytonaTickOptions = { name: 'daytona_tick' };
const daytonaTickColumns = {
  scheduledId: t.u64().primaryKey(),
  scheduledAt: t.scheduleAt(),
};
// A host procedure can only schedule a table registered in its own schema.
export const daytonaTick: ReturnType<
  typeof table<typeof daytonaTickColumns, typeof daytonaTickOptions>
> = table(daytonaTickOptions, daytonaTickColumns);

export type Sandbox = Infer<typeof sandbox.rowType>;
export type Execution = Infer<typeof execution.rowType>;
