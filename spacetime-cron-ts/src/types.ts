import type { Timestamp } from 'spacetimedb';
import type {
  Infer,
  ModuleExport,
  ScheduleAt,
  SenderError,
  t,
  table,
  toCamelCase,
  VariantsObj,
} from 'spacetimedb/server';

export type CronSchedule =
  | { tag: 'cron'; value: { expression: string; timezone: string } }
  | { tag: 'every'; value: { seconds: number } };

/** A cron expression or a fixed interval in seconds. */
export type ScheduleSpec = string | { everySeconds: number };

interface SchedulePolicyOpts {
  /** IANA timezone for cron expressions. Defaults to `UTC`. */
  timezone?: string;
  /** Consecutive failures before automatic disablement. `0` disables this policy. */
  maxFailures?: number;
}

/** Schedule policy plus the durable arguments required by an argument-bearing job. */
export type ScheduleOpts<Args = undefined> = SchedulePolicyOpts &
  ([Args] extends [undefined] ? { args?: never } : { args: Args });

/** Any SpacetimeDB type builder accepted as a cron argument payload. */
export type CronArgsBuilder = VariantsObj[string];

export interface CronTableOpts<Name extends string = string> {
  /** Stable snake_case job name. */
  name: Name;
}

export interface CronTableWithArgsOpts<
  Name extends string = string,
  ArgsBuilder extends CronArgsBuilder = CronArgsBuilder,
> extends CronTableOpts<Name> {
  /** Typed payload persisted with the schedule and copied into each invocation. */
  args: ArgsBuilder;
}

export interface CreateCronOpts {
  /** Completed run records retained per job. Defaults to `5`. */
  historyCap?: number;
  /** Expose trigger and run tables to subscriptions. Defaults to `false`. */
  publicTables?: boolean;
  /**
   * Optional native interval that repairs enabled jobs with missing triggers.
   * Management operations always run the same repair opportunistically.
   */
  reconcileEverySeconds?: number;
}

/** Structural view of the SpacetimeDB timestamp supplied to handlers. */
export interface CronTimestamp {
  readonly microsSinceUnixEpoch: bigint;
  toISOString(): string;
  toDate(): Date;
  toMillis(): bigint;
}

/** Stable metadata supplied to every cron invocation. */
export interface CronInvocation {
  /** Unique across every generation of every job. */
  readonly id: string;
  readonly jobName: string;
  readonly generation: bigint;
  readonly sequence: bigint;
  /** Logical calendar occurrence or interval fire time. */
  readonly scheduledFor: CronTimestamp;
}

export type CronReducerHandler<Ctx, Args = undefined> = [Args] extends [
  undefined,
]
  ? (ctx: Ctx, invocation: CronInvocation) => void
  : (ctx: Ctx, args: Args, invocation: CronInvocation) => void;

export type CronProcedureHandler<Ctx, Args = undefined> = [Args] extends [
  undefined,
]
  ? (ctx: Ctx, invocation: CronInvocation) => void
  : (ctx: Ctx, args: Args, invocation: CronInvocation) => void;

export type CronTableDefinition = ReturnType<typeof table>;
export interface CronSchema {
  readonly anonymousView: (...args: never[]) => ModuleExport;
  readonly reducer: (...args: never[]) => ModuleExport;
  readonly procedure: (...args: never[]) => ModuleExport;
}

export interface CronJobReference<Name extends string = string> {
  readonly jobName: Name;
}

export interface CronJobHandle<Name extends string = string, Args = undefined>
  extends CronJobReference<Name> {
  cronReducer<Ctx>(
    spacetimedb: CronSchema,
    handler: CronReducerHandler<Ctx, Args>
  ): ModuleExport;
  cronProcedure<Ctx>(
    spacetimedb: CronSchema,
    handler: CronProcedureHandler<Ctx, Args>
  ): ModuleExport;
}

export interface CronCorePublicViews {
  /** Sanitized job state. Typed application arguments remain private. */
  readonly jobs: ModuleExport;
}

export interface CronCore {
  /** Spread into the consumer's `schema()` call. */
  readonly tables: Record<string, CronTableDefinition>;
  /** Register and export the optional lost-trigger reconciliation sweep. */
  reconcileReducer(spacetimedb: CronSchema): ModuleExport;
  /** Register the optional public job-state view exactly once. */
  publicViews(spacetimedb: CronSchema): CronCorePublicViews;
}

export interface CronSdk {
  /** Consumer module's `table` value from `spacetimedb/server`. */
  table: typeof table;
  /** Consumer module's `t` value from `spacetimedb/server`. */
  t: typeof t;
  /** Consumer module's `toCamelCase` value from `spacetimedb/server`. */
  toCamelCase: typeof toCamelCase;
  /** Consumer module's `ScheduleAt` value from `spacetimedb`. */
  ScheduleAt: typeof ScheduleAt;
  /** Consumer module's `Timestamp` value from `spacetimedb`. */
  Timestamp: typeof Timestamp;
  /** Consumer module's `SenderError` value from `spacetimedb/server`. */
  SenderError: typeof SenderError;
}

export interface CronApi {
  cronTable<const Name extends string, ArgsBuilder extends CronArgsBuilder>(
    opts: CronTableWithArgsOpts<Name, ArgsBuilder>
  ): CronJobHandle<Name, Infer<ArgsBuilder>>;
  cronTable<const Name extends string>(
    opts: CronTableOpts<Name>
  ): CronJobHandle<Name>;
  createCron(
    jobs: readonly CronJobReference[],
    opts?: CreateCronOpts
  ): CronCore;
  /** Create or replace a job schedule and arm its first trigger. */
  schedule<Ctx, Name extends string, Args>(
    ctx: Ctx,
    job: CronJobHandle<Name, Args>,
    spec: ScheduleSpec,
    ...options: [Args] extends [undefined]
      ? [opts?: ScheduleOpts]
      : [opts: ScheduleOpts<NoInfer<Args>>]
  ): void;
  /** Disable a job and remove its pending trigger. */
  unschedule<Ctx>(ctx: Ctx, job: CronJobReference): void;
}
