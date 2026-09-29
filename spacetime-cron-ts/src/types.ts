import type { Timestamp } from 'spacetimedb';
import type {
  ModuleExport,
  toCamelCase,
  VariantsObj,
} from 'spacetimedb/server';
import type {
  CronFireTableDef,
  CronJobTableDef,
  CronProcedureCtx,
  CronReconcileTickTableDef,
  CronRunTableDef,
  CronTx,
  FireRow,
} from './tables';

export type { CronProcedureCtx, CronTx } from './tables';

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

export interface CronConfig {
  /** Every job handle in the module, each created by `cronTable()`. */
  jobs: readonly CronJobReference[];
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

/** Stable metadata supplied to every cron invocation. */
export interface CronInvocation {
  /** Unique across every generation of every job. */
  readonly id: string;
  readonly jobName: string;
  readonly generation: bigint;
  readonly sequence: bigint;
  /** Logical calendar occurrence or interval fire time. */
  readonly scheduledFor: Timestamp;
}

export type CronHandler<Ctx, Args = undefined> = [Args] extends [undefined]
  ? (ctx: Ctx, invocation: CronInvocation) => void
  : (ctx: Ctx, args: Args, invocation: CronInvocation) => void;

/** A scheduled reducer or procedure registered for one job. */
export type CronFunctionExport<Ctx> = ModuleExport &
  ((ctx: Ctx, args: { arg: FireRow }) => unknown);

/**
 * The host schema returned by `schema()`. Handler contexts are inferred from
 * its reducer and procedure exports.
 */
export interface CronSchema<
  Tx extends CronTx = CronTx,
  Proc extends CronProcedureCtx = CronProcedureCtx,
> {
  reducer(...args: unknown[]): CronFunctionExport<Tx>;
  procedure(...args: unknown[]): CronFunctionExport<Proc>;
  anonymousView(...args: unknown[]): ModuleExport;
}

export interface CronJobReference<Name extends string = string> {
  readonly jobName: Name;
}

export interface CronJobHandle<Name extends string = string, Args = undefined>
  extends CronJobReference<Name> {
  cronReducer<Tx extends CronTx, Proc extends CronProcedureCtx>(
    spacetimedb: CronSchema<Tx, Proc>,
    handler: CronHandler<Tx, Args>
  ): CronFunctionExport<Tx>;
  cronProcedure<Tx extends CronTx, Proc extends CronProcedureCtx>(
    spacetimedb: CronSchema<Tx, Proc>,
    handler: CronHandler<Proc, Args>
  ): CronFunctionExport<Proc>;
}

type FireAccessor<Name extends string> = ReturnType<
  typeof toCamelCase<`${Name}_fire`>
>;

/** The tables to spread into the host's `schema()` call. */
export type CronTables<Config extends CronConfig> = {
  readonly cronJob: CronJobTableDef;
  readonly cronRun: CronRunTableDef;
} & {
  readonly [Job in Config['jobs'][number] as FireAccessor<
    Job['jobName']
  >]: CronFireTableDef;
} & (Config extends { reconcileEverySeconds: number }
    ? { readonly cronReconcileTick: CronReconcileTickTableDef }
    : unknown);

export interface CronPublicViews {
  /** Sanitized job state. Typed application arguments remain private. */
  readonly jobs: ModuleExport;
}

export interface CronClient<Config extends CronConfig = CronConfig> {
  /** Spread into the host's `schema()` call. */
  readonly tables: CronTables<Config>;
  /** Register and export the optional lost-trigger reconciliation sweep. */
  reconcileReducer<Tx extends CronTx, Proc extends CronProcedureCtx>(
    spacetimedb: CronSchema<Tx, Proc>
  ): ModuleExport;
  /** Register the optional public job-state view exactly once. */
  publicViews<Tx extends CronTx, Proc extends CronProcedureCtx>(
    spacetimedb: CronSchema<Tx, Proc>
  ): CronPublicViews;
  /** Create or replace a job schedule and arm its first trigger. */
  schedule<Name extends string, Args>(
    ctx: CronTx,
    job: CronJobHandle<Name, Args>,
    spec: ScheduleSpec,
    ...options: [Args] extends [undefined]
      ? [opts?: ScheduleOpts]
      : [opts: ScheduleOpts<NoInfer<Args>>]
  ): void;
  /** Disable a job and remove its pending trigger. */
  unschedule(ctx: CronTx, job: CronJobReference): void;
}
