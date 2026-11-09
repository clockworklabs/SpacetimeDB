import {
  Range,
  ScheduleAt,
  SenderError,
  t,
  table,
  type Infer,
  type ModuleExport,
  type SumBuilder,
  type VariantsObj,
} from 'spacetimedb/server';
import {
  ConnectionId,
  Identity,
  TimeDuration,
  Timestamp,
  Uuid,
} from 'spacetimedb';

const ONE_SECOND_MICROS = 1_000_000n;
const MAX_ATTEMPTS = 10;
const MAX_BACKOFF_SECONDS = 3600;
const MAX_STEP_TIMEOUT_SECONDS = 86_400;
const DEFAULT_STEP_TIMEOUT_SECONDS = 300;
const MAX_STEPS = 10_000;
const MAX_RESULT_LENGTH = 65_536;
const MAX_EVENT_LENGTH = 128;
const MAX_PENDING_EVENTS = 32;
const MAX_ERROR_LENGTH = 2048;
const MAX_ROWS = 1000;
const MAX_BRANCHES = 32;
const PARALLEL_PREFIX = 'parallel:';

/** Codes a caller can receive as a `SenderError`. */
export const errors = {
  notAuthorized: 'workflow.not_authorized',
  invalidEvent: 'workflow.invalid_event',
  tooManyEvents: 'workflow.too_many_events',
  cannotRemoveLastAdmin: 'workflow.cannot_remove_last_admin',
} as const;

/** Errors recorded on a run or in its history. */
export const failures = {
  stepLost: 'workflow.step_lost',
  tooManySteps: 'workflow.too_many_steps',
  resultTooLarge: 'workflow.result_too_large',
  asyncStep: 'workflow.async_step',
  replayMismatch: 'workflow.replay_mismatch',
  tooManyBranches: 'workflow.too_many_branches',
  unsupportedValue: 'workflow.unsupported_value',
} as const;

/** Thrown inside a workflow function when a step gives up. Safe to catch. */
export class WorkflowStepError extends Error {
  constructor(
    readonly step: string,
    message: string
  ) {
    super(message);
    this.name = 'WorkflowStepError';
  }
}

/** Unwinds the workflow function when a run pauses or is no longer current. */
class WorkflowSuspended extends Error {}

type WorkflowStatusBuilder = SumBuilder<
  Record<
    'Running' | 'Waiting' | 'Completed' | 'Failed' | 'Cancelled',
    ReturnType<typeof t.unit>
  >
>;
const status: WorkflowStatusBuilder = t.enum('WorkflowStatus', {
  Running: t.unit(),
  Waiting: t.unit(),
  Completed: t.unit(),
  Failed: t.unit(),
  Cancelled: t.unit(),
});

function runColumns(args: SumBuilder<VariantsObj>) {
  return {
    id: t.u64().primaryKey().autoInc(),
    args,
    status,
    /** Changes when an attempt starts or the run pauses, so older ones stop. */
    generation: t.u64(),
    attempt: t.u32(),
    /** An attempt has started and has not recorded its result. */
    inFlight: t.bool(),
    stepCount: t.u32(),
    lastStep: t.option(t.string()),
    waitingFor: t.option(t.string()),
    pendingEvents: t.array(t.string()),
    error: t.option(t.string()),
    startedBy: t.identity().index(),
    startedAt: t.timestamp(),
    updatedAt: t.timestamp(),
  };
}

const runOptions = { name: 'workflow_run', public: false } as const;
function runTable(
  args: SumBuilder<VariantsObj>
): ReturnType<typeof table<ReturnType<typeof runColumns>, typeof runOptions>> {
  return table(runOptions, runColumns(args));
}

const journalOptions = { name: 'workflow_journal', public: false } as const;
const journalColumns = {
  id: t.u64().primaryKey().autoInc(),
  runId: t.u64().index(),
  index: t.u32(),
  name: t.string(),
  /** The step's JSON result, if it returned one. */
  result: t.option(t.string()),
  /** Why the step gave up, if it did. */
  error: t.option(t.string()),
};
const workflowJournal: ReturnType<
  typeof table<typeof journalColumns, typeof journalOptions>
> = table(journalOptions, journalColumns);

const fireOptions = { name: 'workflow_fire', public: false } as const;
const fireColumns = {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
  runId: t.u64().index(),
  generation: t.u64(),
  /** Set when the fire runs one item of a parallel step. */
  branch: t.option(t.u32()),
};
const workflowFire: ReturnType<
  typeof table<typeof fireColumns, typeof fireOptions>
> = table(fireOptions, fireColumns);

const branchOptions = { name: 'workflow_branch', public: false } as const;
const branchColumns = {
  id: t.u64().primaryKey().autoInc(),
  runId: t.u64().index(),
  branch: t.u32(),
  attempt: t.u32(),
  inFlight: t.bool(),
  done: t.bool(),
  result: t.option(t.string()),
  error: t.option(t.string()),
};
const workflowBranch: ReturnType<
  typeof table<typeof branchColumns, typeof branchOptions>
> = table(branchOptions, branchColumns);

const finishedOptions = { name: 'workflow_finished', public: false } as const;
const finishedColumns = {
  runId: t.u64().primaryKey(),
  finishedAt: t.timestamp().index(),
};
const workflowFinished: ReturnType<
  typeof table<typeof finishedColumns, typeof finishedOptions>
> = table(finishedOptions, finishedColumns);

type HistoryStatusBuilder = SumBuilder<
  Record<'Ok' | 'Failed' | 'GaveUp', ReturnType<typeof t.unit>>
>;
const historyStatus: HistoryStatusBuilder = t.enum('WorkflowHistoryStatus', {
  Ok: t.unit(),
  Failed: t.unit(),
  GaveUp: t.unit(),
});
const historyOptions = { name: 'workflow_history', public: false } as const;
const historyColumns = {
  id: t.u64().primaryKey().autoInc(),
  runId: t.u64(),
  step: t.string(),
  attempt: t.u32(),
  status: historyStatus,
  error: t.option(t.string()),
  ranAt: t.timestamp().index(),
};
const workflowHistory: ReturnType<
  typeof table<typeof historyColumns, typeof historyOptions>
> = table(historyOptions, historyColumns);

const adminOptions = {
  name: 'workflow_admin_identity',
  public: false,
} as const;
const adminColumns = {
  identity: t.identity().primaryKey(),
  addedAt: t.timestamp(),
};
const workflowAdminIdentity: ReturnType<
  typeof table<typeof adminColumns, typeof adminOptions>
> = table(adminOptions, adminColumns);

interface WorkflowTables {
  workflowRun: ReturnType<typeof runTable>;
  workflowJournal: typeof workflowJournal;
  workflowFire: typeof workflowFire;
  workflowBranch: typeof workflowBranch;
  workflowFinished: typeof workflowFinished;
  workflowHistory: typeof workflowHistory;
  workflowAdminIdentity: typeof workflowAdminIdentity;
}

export type WorkflowRunRow = Infer<ReturnType<typeof runTable>['rowType']>;
export type WorkflowJournalRow = Infer<typeof workflowJournal.rowType>;
export type WorkflowFireRow = Infer<typeof workflowFire.rowType>;
export type WorkflowBranchRow = Infer<typeof workflowBranch.rowType>;
export type WorkflowFinishedRow = Infer<typeof workflowFinished.rowType>;
export type WorkflowHistoryRow = Infer<typeof workflowHistory.rowType>;
type AdminRow = Infer<typeof workflowAdminIdentity.rowType>;

/** The workflow tables of a host view context. */
export interface WorkflowViewCtx {
  readonly sender: Identity;
  readonly db: {
    readonly workflowRun: { iter(): Iterable<WorkflowRunRow> };
    readonly workflowHistory: {
      readonly ranAt: {
        filter(range: Range<Timestamp>): Iterable<WorkflowHistoryRow>;
      };
    };
    readonly workflowAdminIdentity: {
      readonly identity: { find(identity: Identity): AdminRow | null };
    };
  };
}

/** A host transaction context whose schema includes `workflows.tables`. */
export interface WorkflowCtx {
  readonly sender: Identity;
  readonly timestamp: Timestamp;
  readonly db: {
    readonly workflowRun: {
      iter(): Iterable<WorkflowRunRow>;
      readonly id: {
        find(id: bigint): WorkflowRunRow | null;
        update(row: WorkflowRunRow): WorkflowRunRow;
        delete(id: bigint): boolean;
      };
      insert(row: WorkflowRunRow): WorkflowRunRow;
    };
    readonly workflowJournal: {
      readonly runId: {
        filter(runId: bigint): IteratorObject<WorkflowJournalRow, undefined>;
      };
      insert(row: WorkflowJournalRow): WorkflowJournalRow;
      delete(row: WorkflowJournalRow): boolean;
    };
    readonly workflowFire: {
      readonly runId: {
        filter(runId: bigint): IteratorObject<WorkflowFireRow, undefined>;
      };
      insert(row: WorkflowFireRow): WorkflowFireRow;
      delete(row: WorkflowFireRow): boolean;
    };
    readonly workflowBranch: {
      readonly id: {
        update(row: WorkflowBranchRow): WorkflowBranchRow;
      };
      readonly runId: {
        filter(runId: bigint): IteratorObject<WorkflowBranchRow, undefined>;
      };
      insert(row: WorkflowBranchRow): WorkflowBranchRow;
      delete(row: WorkflowBranchRow): boolean;
    };
    readonly workflowFinished: {
      readonly finishedAt: {
        filter(
          range: Range<Timestamp>
        ): IteratorObject<WorkflowFinishedRow, undefined>;
      };
      insert(row: WorkflowFinishedRow): WorkflowFinishedRow;
      delete(row: WorkflowFinishedRow): boolean;
      count(): bigint;
    };
    readonly workflowHistory: {
      readonly ranAt: {
        filter(
          range: Range<Timestamp>
        ): IteratorObject<WorkflowHistoryRow, undefined>;
      };
      insert(row: WorkflowHistoryRow): WorkflowHistoryRow;
      delete(row: WorkflowHistoryRow): boolean;
      count(): bigint;
    };
    readonly workflowAdminIdentity: {
      readonly identity: { find(identity: Identity): AdminRow | null };
      insert(row: AdminRow): AdminRow;
      delete(row: AdminRow): boolean;
      count(): bigint;
    };
  };
}

/** A host procedure context whose schema includes `workflows.tables`. */
export interface WorkflowProcedureCtx<Tx extends WorkflowCtx = WorkflowCtx> {
  readonly sender: Identity;
  readonly databaseIdentity: Identity;
  withTx<T>(body: (tx: Tx) => T): T;
}

export interface WorkflowDefinition {
  /** The run's typed arguments. Part of the module's schema. */
  args: VariantsObj[string];
  /** Attempts per step. Defaults to one attempt with no retry. */
  retry?: { maxAttempts: number; backoffSeconds: number };
  /**
   * How long to wait for an attempt before treating it as lost and starting
   * another. Set it above the slowest `call` step. Defaults to 300.
   */
  stepTimeoutSeconds?: number;
}

export type WorkflowDefinitions = Record<string, WorkflowDefinition>;

/** Identifies the current attempt of a step. */
export interface WorkflowStepInfo {
  readonly runId: bigint;
  readonly step: string;
  /** 1 for the first attempt. */
  readonly attempt: number;
  /** The same for every attempt of this step in this run. */
  readonly idempotencyKey: string;
  readonly startedBy: Identity;
}

/** Runs and remembers steps. Passed to every workflow function. */
export interface WorkflowSteps<Tx, Proc> {
  /**
   * Run `fn` in one transaction and save its result. Its writes and the saved
   * result commit together. `fn` must not call outside services.
   */
  tx<T>(name: string, fn: (ctx: Tx, run: WorkflowStepInfo) => T): T;
  /** Run `fn` in a procedure and save its result. It can run more than once. */
  call<T>(name: string, fn: (ctx: Proc, run: WorkflowStepInfo) => T): T;
  /** Pause for `seconds`. Nothing runs in the meantime. */
  sleep(seconds: number): void;
  /** Pause until `signal` delivers `event`. */
  waitFor(event: string): void;
  /**
   * Run `fn` once per item, each in its own procedure, at the same time. Waits
   * for all of them and returns their outcomes in item order. Each item retries
   * like a `call` step. Up to 32 items.
   */
  parallel<I, T>(
    name: string,
    items: readonly I[],
    fn: (ctx: Proc, item: I, run: WorkflowStepInfo) => T
  ): WorkflowSettled<T>[];
}

/** The outcome of one item of a parallel step. */
export type WorkflowSettled<T> =
  | { ok: true; value: T }
  | { ok: false; error: string };

/** One function per workflow. It must make the same step calls on every rerun. */
export type WorkflowHandlers<Defs extends WorkflowDefinitions, Tx, Proc> = {
  [Name in keyof Defs & string]: (
    step: WorkflowSteps<Tx, Proc>,
    args: Infer<Defs[Name]['args']>
  ) => void;
};

/** The scheduled function that runs workflows. */
export type WorkflowFunctionExport<Ctx> = ModuleExport &
  ((ctx: Ctx, args: { arg: WorkflowFireRow }) => unknown);

/**
 * The host schema returned by `schema()`. Handler contexts are inferred from
 * its reducer and procedure exports.
 */
export interface WorkflowSchema<
  Tx extends WorkflowCtx = WorkflowCtx,
  Proc extends WorkflowProcedureCtx<Tx> = WorkflowProcedureCtx<Tx>,
> {
  reducer(...args: unknown[]): WorkflowFunctionExport<Tx>;
  procedure(...args: unknown[]): WorkflowFunctionExport<Proc>;
}

type InternalHandler = (step: unknown, args: unknown) => void;

function errorText(error: unknown): string {
  const text = error instanceof Error ? error.message : String(error);
  return text.slice(0, MAX_ERROR_LENGTH);
}

// SpacetimeDB values, found by their single bigint field rather than
// instanceof, because the server and client bundles each have their own copy
// of these classes. They come back as the classes exported by 'spacetimedb'.
const SPACETIME_TYPES = [
  ['$timestamp', Timestamp, '__timestamp_micros_since_unix_epoch__'],
  ['$duration', TimeDuration, '__time_duration_micros__'],
  ['$identity', Identity, '__identity__'],
  ['$connectionId', ConnectionId, '__connection_id__'],
  ['$uuid', Uuid, '__uuid__'],
] as const;

function typeName(value: unknown): string {
  return (value as object).constructor?.name ?? typeof value;
}

/** Encodes a step result, rejecting values that would not come back the same. */
function encodeResult(value: unknown): string | undefined {
  if (
    typeof (value as { then?: unknown } | null | undefined)?.then === 'function'
  ) {
    throw new SenderError(failures.asyncStep);
  }
  const json = JSON.stringify(
    value,
    function (this: Record<string, unknown>, key: string) {
      // The raw value, before JSON.stringify applies any toJSON method.
      const item = this[key];
      if (typeof item === 'bigint') return { $bigint: item.toString() };
      if (typeof item === 'number' && !Number.isFinite(item)) {
        throw new SenderError(`${failures.unsupportedValue}:${item}`);
      }
      if (typeof item === 'function' || typeof item === 'symbol') {
        throw new SenderError(`${failures.unsupportedValue}:${typeof item}`);
      }
      if (item === null || typeof item !== 'object') return item;
      if (Array.isArray(item)) return item;
      const fields = Object.keys(item);
      const only = (item as Record<string, unknown>)[fields[0]];
      if (fields.length === 1 && typeof only === 'bigint') {
        const known = SPACETIME_TYPES.find(
          ([, , field]) => field === fields[0]
        );
        if (known) return { [known[0]]: only.toString() };
      }
      const proto = Object.getPrototypeOf(item);
      if (proto === Object.prototype || proto === null) return item;
      if (item instanceof Uint8Array && proto === Uint8Array.prototype) {
        return {
          $bytes: Array.from(item, b => b.toString(16).padStart(2, '0')).join(
            ''
          ),
        };
      }
      throw new SenderError(`${failures.unsupportedValue}:${typeName(item)}`);
    }
  );
  if (json !== undefined && json.length > MAX_RESULT_LENGTH) {
    throw new SenderError(failures.resultTooLarge);
  }
  return json;
}

function decodeResult(json: string | undefined): unknown {
  if (json === undefined) return undefined;
  return JSON.parse(json, (_key, item) => {
    if (item === null || typeof item !== 'object' || Array.isArray(item)) {
      return item;
    }
    if (typeof item.$bigint === 'string') return BigInt(item.$bigint);
    if (typeof item.$bytes === 'string') {
      return Uint8Array.from(item.$bytes.match(/../g) ?? [], (h: string) =>
        parseInt(h, 16)
      );
    }
    for (const [tag, type] of SPACETIME_TYPES) {
      if (typeof item[tag] === 'string') return new type(BigInt(item[tag]));
    }
    return item;
  });
}

function validate(name: string, def: WorkflowDefinition): void {
  const fail = (detail: string): never => {
    throw new Error(`workflow.invalid_definition:${name}:${detail}`);
  };
  const { maxAttempts, backoffSeconds } = def.retry ?? {
    maxAttempts: 1,
    backoffSeconds: 1,
  };
  if (
    !Number.isInteger(maxAttempts) ||
    maxAttempts < 1 ||
    maxAttempts > MAX_ATTEMPTS
  ) {
    fail('max_attempts');
  }
  if (
    !Number.isInteger(backoffSeconds) ||
    backoffSeconds < 1 ||
    backoffSeconds > MAX_BACKOFF_SECONDS
  ) {
    fail('backoff_seconds');
  }
  const timeout = def.stepTimeoutSeconds ?? DEFAULT_STEP_TIMEOUT_SECONDS;
  if (
    !Number.isInteger(timeout) ||
    timeout < 1 ||
    timeout > MAX_STEP_TIMEOUT_SECONDS
  ) {
    fail('step_timeout_seconds');
  }
}

/** Create the workflow tables and the functions that manage them. */
export function client<const Defs extends WorkflowDefinitions>({
  workflows,
}: {
  workflows: Defs;
}) {
  const defs: WorkflowDefinitions = workflows;
  const variants: VariantsObj = {};
  for (const [name, def] of Object.entries(defs)) {
    validate(name, def);
    variants[name] = def.args;
  }
  const workflowRun = runTable(t.enum('WorkflowArgs', variants));
  const tables: WorkflowTables = {
    workflowRun,
    workflowJournal,
    workflowFire,
    workflowBranch,
    workflowFinished,
    workflowHistory,
    workflowAdminIdentity,
  };

  let handlers: Record<string, InternalHandler> | undefined;

  const stepTimeoutMicros = (run: WorkflowRunRow): bigint =>
    BigInt(
      defs[run.args.tag].stepTimeoutSeconds ?? DEFAULT_STEP_TIMEOUT_SECONDS
    ) * ONE_SECOND_MICROS;

  /** Remove a run's fires, or only one parallel item's. */
  function clearFires(ctx: WorkflowCtx, runId: bigint, branch?: number): void {
    for (const fire of [...ctx.db.workflowFire.runId.filter(runId)]) {
      if (branch === undefined || fire.branch === branch) {
        ctx.db.workflowFire.delete(fire);
      }
    }
  }

  function insertFire(
    ctx: WorkflowCtx,
    run: WorkflowRunRow,
    at: bigint,
    branch?: number
  ): void {
    ctx.db.workflowFire.insert({
      scheduledId: 0n,
      scheduledAt: ScheduleAt.time(at),
      runId: run.id,
      generation: run.generation,
      branch,
    });
  }

  /** Keep one fire that recovers the current attempt if the host stops. */
  function replaceGuard(ctx: WorkflowCtx, run: WorkflowRunRow): void {
    clearFires(ctx, run.id);
    insertFire(
      ctx,
      run,
      ctx.timestamp.microsSinceUnixEpoch + stepTimeoutMicros(run)
    );
  }

  /**
   * The host deletes a scheduled procedure's row before calling it. The second
   * row keeps the run alive if the host stops before the first one is claimed.
   */
  function schedule(
    ctx: WorkflowCtx,
    run: WorkflowRunRow,
    delayMicros: bigint,
    branch?: number
  ): void {
    const at = ctx.timestamp.microsSinceUnixEpoch + delayMicros;
    insertFire(ctx, run, at, branch);
    insertFire(ctx, run, at + stepTimeoutMicros(run), branch);
  }

  function recordAttempt(
    ctx: WorkflowCtx,
    run: WorkflowRunRow,
    step: string,
    tag: 'Ok' | 'Failed' | 'GaveUp',
    error?: string
  ): void {
    const history = ctx.db.workflowHistory;
    // Prune before inserting: an index scan yields rows inserted in the
    // current transaction before committed rows.
    if (history.count() >= MAX_ROWS) {
      const oldest = history.ranAt.filter(new Range<Timestamp>()).next().value;
      if (oldest) history.delete(oldest);
    }
    history.insert({
      id: 0n,
      runId: run.id,
      step,
      attempt: run.attempt,
      status: { tag },
      error,
      ranAt: ctx.timestamp,
    });
  }

  function finish(
    ctx: WorkflowCtx,
    run: WorkflowRunRow,
    tag: 'Completed' | 'Failed' | 'Cancelled',
    error: string | undefined
  ): void {
    const finished = ctx.db.workflowFinished;
    if (finished.count() >= MAX_ROWS) {
      const oldest = finished.finishedAt
        .filter(new Range<Timestamp>())
        .next().value;
      if (oldest) {
        finished.delete(oldest);
        ctx.db.workflowRun.id.delete(oldest.runId);
      }
    }
    clearFires(ctx, run.id);
    for (const entry of [...ctx.db.workflowJournal.runId.filter(run.id)]) {
      ctx.db.workflowJournal.delete(entry);
    }
    for (const branch of [...ctx.db.workflowBranch.runId.filter(run.id)]) {
      ctx.db.workflowBranch.delete(branch);
    }
    ctx.db.workflowRun.id.update({
      ...run,
      status: { tag },
      generation: run.generation + 1n,
      inFlight: false,
      waitingFor: undefined,
      error,
      updatedAt: ctx.timestamp,
    });
    finished.insert({ runId: run.id, finishedAt: ctx.timestamp });
  }

  function appendJournal(
    ctx: WorkflowCtx,
    run: WorkflowRunRow,
    name: string,
    entry: { result?: string; error?: string }
  ): void {
    ctx.db.workflowJournal.insert({
      id: 0n,
      runId: run.id,
      index: run.stepCount,
      name,
      result: entry.result,
      error: entry.error,
    });
  }

  /** Save a finished step and carry on to the next one in this attempt. */
  function commitStep(
    ctx: WorkflowCtx,
    run: WorkflowRunRow,
    name: string,
    entry: { result?: string; error?: string }
  ): void {
    appendJournal(ctx, run, name, entry);
    const updated = ctx.db.workflowRun.id.update({
      ...run,
      attempt: 1,
      stepCount: run.stepCount + 1,
      lastStep: name,
      updatedAt: ctx.timestamp,
    });
    replaceGuard(ctx, updated);
  }

  /** Stop this attempt and run the function again after `delayMicros`. */
  function pause(
    ctx: WorkflowCtx,
    run: WorkflowRunRow,
    changes: Partial<WorkflowRunRow>,
    delayMicros: bigint
  ): void {
    clearFires(ctx, run.id);
    const moved = ctx.db.workflowRun.id.update({
      ...run,
      ...changes,
      status: { tag: 'Running' },
      generation: run.generation + 1n,
      inFlight: false,
      waitingFor: undefined,
      updatedAt: ctx.timestamp,
    });
    schedule(ctx, moved, delayMicros);
  }

  /** Start an attempt by loading the steps that already finished. */
  function claim(ctx: WorkflowCtx, arg: WorkflowFireRow) {
    const run = ctx.db.workflowRun.id.find(arg.runId);
    if (run?.status.tag !== 'Running' || run.generation !== arg.generation) {
      return undefined;
    }
    const claimed = ctx.db.workflowRun.id.update({
      ...run,
      generation: run.generation + 1n,
      attempt: run.attempt + 1,
      inFlight: true,
      updatedAt: ctx.timestamp,
    });
    replaceGuard(ctx, claimed);
    return {
      run: claimed,
      lost: run.inFlight,
      journal: [...ctx.db.workflowJournal.runId.filter(run.id)].sort(
        (a, b) => a.index - b.index
      ),
    };
  }

  function retryDelayMicros(run: WorkflowRunRow, attempt: number): bigint {
    return (
      BigInt(defs[run.args.tag].retry?.backoffSeconds ?? 1) *
      (1n << BigInt(attempt - 1)) *
      ONE_SECOND_MICROS
    );
  }

  /** The run, if it is still waiting on this parallel step. */
  function waitingOnBranches(
    ctx: WorkflowCtx,
    runId: bigint,
    generation: bigint
  ): WorkflowRunRow | undefined {
    const run = ctx.db.workflowRun.id.find(runId);
    return run?.status.tag === 'Waiting' &&
      run.generation === generation &&
      run.waitingFor?.startsWith(PARALLEL_PREFIX)
      ? run
      : undefined;
  }

  /** Record one item's outcome. The last item saves them all and wakes the run. */
  function settleBranch(
    ctx: WorkflowCtx,
    runId: bigint,
    generation: bigint,
    branch: number,
    outcome: { result?: string; error?: string }
  ): void {
    const run = waitingOnBranches(ctx, runId, generation);
    if (!run) return;
    const rows = [...ctx.db.workflowBranch.runId.filter(runId)];
    const row = rows.find(b => b.branch === branch);
    if (!row || row.done) return;
    clearFires(ctx, runId, branch);
    const label = `${run.waitingFor!.slice(PARALLEL_PREFIX.length)} #${branch + 1}`;
    recordAttempt(
      ctx,
      { ...run, attempt: row.attempt },
      label,
      outcome.error === undefined ? 'Ok' : 'GaveUp',
      outcome.error
    );
    const settled = ctx.db.workflowBranch.id.update({
      ...row,
      inFlight: false,
      done: true,
      ...outcome,
    });
    const all = rows.map(b => (b.branch === branch ? settled : b));
    if (!all.every(b => b.done)) return;

    const results = all
      .sort((a, b) => a.branch - b.branch)
      .map(b =>
        b.error !== undefined
          ? JSON.stringify({ ok: false, error: b.error })
          : b.result === undefined
            ? '{"ok":true}'
            : `{"ok":true,"value":${b.result}}`
      );
    for (const b of all) ctx.db.workflowBranch.delete(b);
    const name = run.waitingFor!;
    appendJournal(ctx, run, name, { result: `[${results.join(',')}]` });
    pause(
      ctx,
      run,
      { attempt: 0, stepCount: run.stepCount + 1, lastStep: name },
      0n
    );
  }

  /** Run one item of a parallel step, by replaying the function up to it. */
  function runBranch<Tx extends WorkflowCtx>(
    ctx: WorkflowProcedureCtx<Tx>,
    arg: WorkflowFireRow,
    branch: number
  ): void {
    const claimed = ctx.withTx(tx => {
      const run = waitingOnBranches(tx, arg.runId, arg.generation);
      const row = run
        ? [...tx.db.workflowBranch.runId.filter(run.id)].find(
            b => b.branch === branch
          )
        : undefined;
      if (!run || !row || row.done) return undefined;
      clearFires(tx, run.id, branch);
      const updated = tx.db.workflowBranch.id.update({
        ...row,
        attempt: row.attempt + 1,
        inFlight: true,
      });
      insertFire(
        tx,
        run,
        tx.timestamp.microsSinceUnixEpoch + stepTimeoutMicros(run),
        branch
      );
      return {
        run,
        row: updated,
        lost: row.inFlight,
        journal: [...tx.db.workflowJournal.runId.filter(run.id)].sort(
          (a, b) => a.index - b.index
        ),
      };
    });
    if (!claimed) return;
    const { run, row, journal } = claimed;
    const maxAttempts = defs[run.args.tag].retry?.maxAttempts ?? 1;
    const settle = (outcome: { result?: string; error?: string }) =>
      ctx.withTx(tx =>
        settleBranch(tx, run.id, run.generation, branch, outcome)
      );

    if (claimed.lost) {
      if (row.attempt > maxAttempts) {
        settle({ error: failures.stepLost });
        return;
      }
      ctx.withTx(tx => {
        if (waitingOnBranches(tx, run.id, run.generation)) {
          recordAttempt(
            tx,
            { ...run, attempt: row.attempt - 1 },
            `${run.waitingFor!.slice(PARALLEL_PREFIX.length)} #${branch + 1}`,
            'Failed',
            failures.stepLost
          );
        }
      });
    }

    let cursor = 0;
    let done = false;
    const stop = (): never => {
      done = true;
      throw new WorkflowSuspended();
    };
    // Finished steps return their saved results. Anything else means the code
    // no longer matches this run, so this item gives up.
    const replay = (name: string): unknown => {
      if (done) stop();
      const saved = journal[cursor++];
      if (!saved || saved.name !== name) {
        settle({ error: failures.replayMismatch });
        stop();
      }
      if (saved!.error !== undefined) {
        throw new WorkflowStepError(name, saved!.error);
      }
      return decodeResult(saved!.result);
    };
    const steps: WorkflowSteps<Tx, WorkflowProcedureCtx<Tx>> = {
      tx: <T>(name: string) => replay(name) as T,
      call: <T>(name: string) => replay(name) as T,
      sleep: () => void replay('sleep'),
      waitFor: (event: string) => void replay(`wait:${event}`),
      parallel<I, T>(
        name: string,
        items: readonly I[],
        fn: (ctx: WorkflowProcedureCtx<Tx>, item: I, run: WorkflowStepInfo) => T
      ): WorkflowSettled<T>[] {
        if (done) stop();
        const index = cursor;
        if (index < journal.length) {
          return replay(`${PARALLEL_PREFIX}${name}`) as WorkflowSettled<T>[];
        }
        if (
          index !== run.stepCount ||
          `${PARALLEL_PREFIX}${name}` !== run.waitingFor ||
          branch >= items.length
        ) {
          settle({ error: failures.replayMismatch });
          return stop();
        }
        const info: WorkflowStepInfo = {
          runId: run.id,
          step: `${name} #${branch + 1}`,
          attempt: row.attempt,
          idempotencyKey: `${run.id}:${index}:${branch}`,
          startedBy: run.startedBy,
        };
        try {
          settle({ result: encodeResult(fn(ctx, items[branch], info)) });
        } catch (caught) {
          if (caught instanceof WorkflowSuspended) throw caught;
          const error = errorText(caught);
          if (caught instanceof SenderError || row.attempt >= maxAttempts) {
            settle({ error });
          } else {
            ctx.withTx(tx => {
              if (!waitingOnBranches(tx, run.id, run.generation)) return;
              recordAttempt(
                tx,
                { ...run, attempt: row.attempt },
                info.step,
                'Failed',
                error
              );
              const current = [
                ...tx.db.workflowBranch.runId.filter(run.id),
              ].find(b => b.branch === branch);
              if (current && !current.done) {
                tx.db.workflowBranch.id.update({ ...current, inFlight: false });
              }
              clearFires(tx, run.id, branch);
              schedule(tx, run, retryDelayMicros(run, row.attempt), branch);
            });
          }
        }
        return stop();
      },
    };

    const args = 'value' in run.args ? run.args.value : undefined;
    try {
      handlers![run.args.tag](steps, args);
    } catch {
      // The function stops at this item, or failed before reaching it.
    }
    if (!done) settle({ error: failures.replayMismatch });
  }

  function runAttempt<Tx extends WorkflowCtx>(
    ctx: WorkflowProcedureCtx<Tx>,
    arg: WorkflowFireRow
  ): void {
    if (!ctx.sender.isEqual(ctx.databaseIdentity)) {
      throw new SenderError(errors.notAuthorized);
    }
    if (arg.branch !== undefined) {
      runBranch(ctx, arg, arg.branch);
      return;
    }
    const loaded = ctx.withTx(tx => claim(tx, arg));
    if (!loaded) return;
    const { journal } = loaded;
    const { id: runId, generation, startedBy } = loaded.run;
    const tag = loaded.run.args.tag;
    const def = defs[tag];
    const maxAttempts = def.retry?.maxAttempts ?? 1;
    let attempt = loaded.run.attempt;
    let lost = loaded.lost;
    let cursor = 0;
    let suspended = false;

    const current = (tx: WorkflowCtx): WorkflowRunRow | undefined => {
      const run = tx.db.workflowRun.id.find(runId);
      return run?.status.tag === 'Running' && run.generation === generation
        ? run
        : undefined;
    };

    // Remembered so a function that catches the signal still stops here.
    const suspend = (): never => {
      suspended = true;
      throw new WorkflowSuspended();
    };

    /** Commit in a transaction, or stop if this attempt is no longer current. */
    const commit = <T>(body: (tx: Tx, run: WorkflowRunRow) => T): T => {
      const committed = ctx.withTx(tx => {
        const run = current(tx);
        return run ? { value: body(tx, run) } : undefined;
      });
      return committed ? committed.value : suspend();
    };

    const failRun = (error: string): never => {
      commit((tx, run) => finish(tx, run, 'Failed', error));
      return suspend();
    };

    /** Returns the saved outcome of a step, if it already finished. */
    const replay = (name: string): { value: unknown } | undefined => {
      if (suspended) suspend();
      const index = cursor++;
      const saved = journal[index];
      if (!saved) {
        if (index >= MAX_STEPS) failRun(failures.tooManySteps);
        return undefined;
      }
      if (saved.name !== name) {
        failRun(`${failures.replayMismatch}:${index}:${saved.name}:${name}`);
      }
      if (saved.error !== undefined) {
        throw new WorkflowStepError(name, saved.error);
      }
      return { value: decodeResult(saved.result) };
    };

    const giveUp = (name: string, error: string): never => {
      commit((tx, run) => {
        recordAttempt(tx, run, name, 'GaveUp', error);
        commitStep(tx, run, name, { error });
      });
      attempt = 1;
      throw new WorkflowStepError(name, error);
    };

    /** The first step that has not finished, after a lost attempt. */
    const accountForLostAttempt = (name: string, limit: number): void => {
      if (!lost) return;
      lost = false;
      if (attempt > limit) giveUp(name, failures.stepLost);
      commit((tx, run) => {
        recordAttempt(tx, run, name, 'Failed', failures.stepLost);
      });
    };

    const handleFailure = (name: string, caught: unknown): never => {
      if (caught instanceof WorkflowSuspended) throw caught;
      const error = errorText(caught);
      // A `SenderError` rejects the step's input, so retrying cannot help.
      if (caught instanceof SenderError || attempt >= maxAttempts) {
        giveUp(name, error);
      }
      commit((tx, run) => {
        recordAttempt(tx, run, name, 'Failed', error);
        pause(tx, run, {}, retryDelayMicros(run, attempt));
      });
      return suspend();
    };

    const info = (name: string): WorkflowStepInfo => ({
      runId,
      step: name,
      attempt,
      idempotencyKey: `${runId}:${cursor - 1}`,
      startedBy,
    });

    const finishStep = (
      tx: WorkflowCtx,
      run: WorkflowRunRow,
      name: string,
      result: string | undefined
    ): void => {
      recordAttempt(tx, run, name, 'Ok');
      commitStep(tx, run, name, { result });
    };

    const steps: WorkflowSteps<Tx, WorkflowProcedureCtx<Tx>> = {
      tx<T>(name: string, fn: (tx: Tx, run: WorkflowStepInfo) => T): T {
        const saved = replay(name);
        if (saved) return saved.value as T;
        // Nothing commits if the host stops during a transaction, so a lost
        // attempt is safe to repeat up to the overall limit.
        accountForLostAttempt(name, MAX_ATTEMPTS);
        const stepInfo = info(name);
        try {
          const result = commit((tx, run) => {
            const encoded = encodeResult(fn(tx, stepInfo));
            finishStep(tx, run, name, encoded);
            return encoded;
          });
          attempt = 1;
          return decodeResult(result) as T;
        } catch (caught) {
          return handleFailure(name, caught);
        }
      },
      call<T>(
        name: string,
        fn: (ctx: WorkflowProcedureCtx<Tx>, run: WorkflowStepInfo) => T
      ): T {
        const saved = replay(name);
        if (saved) return saved.value as T;
        accountForLostAttempt(name, maxAttempts);
        try {
          const encoded = encodeResult(fn(ctx, info(name)));
          commit((tx, run) => finishStep(tx, run, name, encoded));
          attempt = 1;
          return decodeResult(encoded) as T;
        } catch (caught) {
          return handleFailure(name, caught);
        }
      },
      sleep(seconds: number): void {
        if (replay('sleep')) return;
        const delay = BigInt(Math.ceil(seconds)) * ONE_SECOND_MICROS;
        commit((tx, run) => {
          appendJournal(tx, run, 'sleep', {});
          pause(
            tx,
            run,
            { attempt: 0, stepCount: run.stepCount + 1, lastStep: 'sleep' },
            delay
          );
        });
        suspend();
      },
      waitFor(event: string): void {
        const name = `wait:${event}`;
        if (replay(name)) return;
        const ready = commit((tx, run) => {
          if (run.pendingEvents.includes(event)) {
            const pendingEvents = run.pendingEvents.filter(e => e !== event);
            commitStep(tx, { ...run, pendingEvents }, name, {});
            return true;
          }
          clearFires(tx, run.id);
          tx.db.workflowRun.id.update({
            ...run,
            status: { tag: 'Waiting' },
            generation: run.generation + 1n,
            attempt: 0,
            inFlight: false,
            waitingFor: event,
            updatedAt: tx.timestamp,
          });
          return false;
        });
        if (!ready) suspend();
        attempt = 1;
      },
      parallel<I, T>(name: string, items: readonly I[]): WorkflowSettled<T>[] {
        const label = `${PARALLEL_PREFIX}${name}`;
        const saved = replay(label);
        if (saved) return saved.value as WorkflowSettled<T>[];
        if (items.length > MAX_BRANCHES) {
          throw new Error(failures.tooManyBranches);
        }
        if (items.length === 0) {
          commit((tx, run) => commitStep(tx, run, label, { result: '[]' }));
          attempt = 1;
          return [];
        }
        commit((tx, run) => {
          clearFires(tx, run.id);
          const waiting = tx.db.workflowRun.id.update({
            ...run,
            status: { tag: 'Waiting' },
            generation: run.generation + 1n,
            attempt: 0,
            inFlight: false,
            waitingFor: label,
            updatedAt: tx.timestamp,
          });
          items.forEach((_item, branch) => {
            tx.db.workflowBranch.insert({
              id: 0n,
              runId: run.id,
              branch,
              attempt: 0,
              inFlight: false,
              done: false,
              result: undefined,
              error: undefined,
            });
            schedule(tx, waiting, 0n, branch);
          });
        });
        return suspend();
      },
    };

    const args = 'value' in loaded.run.args ? loaded.run.args.value : undefined;
    let error: string | undefined;
    try {
      handlers![tag](steps, args);
    } catch (caught) {
      if (caught instanceof WorkflowSuspended) return;
      error = errorText(caught);
    }
    if (suspended) return;
    ctx.withTx(tx => {
      const run = current(tx);
      if (run) {
        finish(tx, run, error === undefined ? 'Completed' : 'Failed', error);
      }
    });
  }

  function isAdmin(ctx: WorkflowViewCtx): boolean {
    return ctx.db.workflowAdminIdentity.identity.find(ctx.sender) != null;
  }

  function requireAdmin(ctx: WorkflowCtx): void {
    if (!isAdmin(ctx)) throw new SenderError(errors.notAuthorized);
  }

  function addAdmin(ctx: WorkflowCtx, identity: Identity): void {
    if (ctx.db.workflowAdminIdentity.identity.find(identity) == null) {
      ctx.db.workflowAdminIdentity.insert({ identity, addedAt: ctx.timestamp });
    }
  }

  function cancel(ctx: WorkflowCtx, runId: bigint): boolean {
    const run = ctx.db.workflowRun.id.find(runId);
    if (run?.status.tag !== 'Running' && run?.status.tag !== 'Waiting') {
      return false;
    }
    finish(ctx, run, 'Cancelled', run.error);
    return true;
  }

  return {
    tables,
    /**
     * Register the scheduled procedure that runs workflows. Call after
     * `schema()` and export the result.
     */
    register<Tx extends WorkflowCtx, Proc extends WorkflowProcedureCtx<Tx>>(
      spacetimedb: WorkflowSchema<Tx, Proc>,
      workflowHandlers: WorkflowHandlers<Defs, Tx, Proc>
    ): WorkflowFunctionExport<Proc> {
      handlers = workflowHandlers as unknown as typeof handlers;
      return spacetimedb.procedure(
        { name: 'workflow_run_attempt', onSchedule: workflowFire },
        { arg: workflowFire.rowType },
        t.unit(),
        (ctx: Proc, { arg }: { arg: WorkflowFireRow }) => {
          runAttempt(ctx, arg);
          return {};
        }
      );
    },
    /** Call from the host's init reducer to seed the publishing identity as admin. */
    install(ctx: WorkflowCtx): void {
      addAdmin(ctx, ctx.sender);
    },
    /** Start a run and return its id. The caller authorizes. */
    start<Name extends keyof Defs & string>(
      ctx: WorkflowCtx,
      name: Name,
      args: Infer<Defs[Name]['args']>
    ): bigint {
      const run = ctx.db.workflowRun.insert({
        id: 0n,
        args: { tag: name, value: args },
        status: { tag: 'Running' },
        generation: 0n,
        attempt: 0,
        inFlight: false,
        stepCount: 0,
        lastStep: undefined,
        waitingFor: undefined,
        pendingEvents: [],
        error: undefined,
        startedBy: ctx.sender,
        startedAt: ctx.timestamp,
        updatedAt: ctx.timestamp,
      } as WorkflowRunRow);
      schedule(ctx, run, 0n);
      return run.id;
    },
    /**
     * Deliver an event. An active run that is not waiting for it yet keeps it
     * until it does. Returns false if the run is unknown or finished. The caller
     * authorizes.
     */
    signal(ctx: WorkflowCtx, runId: bigint, event: string): boolean {
      if (
        event.length === 0 ||
        event.length > MAX_EVENT_LENGTH ||
        event.startsWith(PARALLEL_PREFIX)
      ) {
        throw new SenderError(errors.invalidEvent);
      }
      const run = ctx.db.workflowRun.id.find(runId);
      if (run?.status.tag !== 'Running' && run?.status.tag !== 'Waiting') {
        return false;
      }
      if (run.waitingFor === event) {
        const name = `wait:${event}`;
        appendJournal(ctx, run, name, {});
        pause(
          ctx,
          run,
          { attempt: 0, stepCount: run.stepCount + 1, lastStep: name },
          0n
        );
        return true;
      }
      if (run.pendingEvents.includes(event)) return true;
      if (run.pendingEvents.length >= MAX_PENDING_EVENTS) {
        throw new SenderError(errors.tooManyEvents);
      }
      ctx.db.workflowRun.id.update({
        ...run,
        pendingEvents: [...run.pendingEvents, event],
      });
      return true;
    },
    /** Stop an active run. Returns false if it already finished. The caller authorizes. */
    cancel,
    requireAdmin,
    views: {
      /** The latest runs, newest first. */
      workflowRunsAdmin(ctx: WorkflowViewCtx): WorkflowRunRow[] {
        if (!isAdmin(ctx)) return [];
        return [...ctx.db.workflowRun.iter()]
          .sort((a, b) => (a.id > b.id ? -1 : a.id < b.id ? 1 : 0))
          .slice(0, MAX_ROWS);
      },
      /** Retained step attempts, newest first. */
      workflowHistoryAdmin(ctx: WorkflowViewCtx): WorkflowHistoryRow[] {
        if (!isAdmin(ctx)) return [];
        return [
          ...ctx.db.workflowHistory.ranAt.filter(new Range<Timestamp>()),
        ].reverse();
      },
    },
    reducers: {
      cancelWorkflowRun: {
        params: { runId: t.u64() },
        handler(ctx: WorkflowCtx, { runId }: { runId: bigint }): void {
          requireAdmin(ctx);
          cancel(ctx, runId);
        },
      },
      addWorkflowAdminIdentity: {
        params: { identity: t.identity() },
        handler(ctx: WorkflowCtx, { identity }: { identity: Identity }): void {
          requireAdmin(ctx);
          addAdmin(ctx, identity);
        },
      },
      removeWorkflowAdminIdentity: {
        params: { identity: t.identity() },
        handler(ctx: WorkflowCtx, { identity }: { identity: Identity }): void {
          requireAdmin(ctx);
          const existing = ctx.db.workflowAdminIdentity.identity.find(identity);
          if (!existing) return;
          if (ctx.db.workflowAdminIdentity.count() <= 1n) {
            throw new SenderError(errors.cannotRemoveLastAdmin);
          }
          ctx.db.workflowAdminIdentity.delete(existing);
        },
      },
    },
  };
}
