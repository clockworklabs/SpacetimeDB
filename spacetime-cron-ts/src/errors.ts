/**
 * Codes a caller can receive at runtime. `schedule()` throws the input codes as
 * `SenderError`s; the disabled-reason codes are stored on the job row. Some are
 * followed by `:` and detail text.
 */
export const errors = {
  notAuthorized: 'cron.not_authorized',
  // Thrown by `schedule()`.
  invalidMaxFailures: 'cron.invalid_max_failures',
  invalidInterval: 'cron.invalid_interval',
  invalidExpression: 'cron.invalid_expression',
  invalidTimezone: 'cron.invalid_timezone',
  unsatisfiableExpression: 'cron.unsatisfiable_expression',
  missingArgs: 'cron.missing_args',
  unexpectedArgs: 'cron.unexpected_args',
  // Stored as a job's disabled reason.
  invalidScheduleState: 'cron.invalid_schedule_state',
  invalidArgsState: 'cron.invalid_args_state',
  invalidTrigger: 'cron.invalid_trigger',
  noFutureOccurrence: 'cron.no_future_occurrence',
} as const;

/**
 * Module-definition and handler programming errors. They fail module loading
 * or an invocation and are not exported from the package.
 */
export const internalErrors = {
  foreignJobHandle: 'cron.foreign_job_handle',
  notWired: 'cron.not_wired',
  noJobs: 'cron.no_jobs',
  duplicateJob: 'cron.duplicate_job',
  jobAlreadyWired: 'cron.job_already_wired',
  tableKeyCollision: 'cron.table_key_collision',
  missingTable: 'cron.missing_table',
  missingHandlers: 'cron.missing_handlers',
  handlerAlreadyRegistered: 'cron.handler_already_registered',
  reconcileNotConfigured: 'cron.reconcile_not_configured',
  reconcileReducerNotRegistered: 'cron.reconcile_reducer_not_registered',
  reconcileReducerAlreadyRegistered:
    'cron.reconcile_reducer_already_registered',
  publicViewsAlreadyRegistered: 'cron.public_views_already_registered',
  asyncReducerHandler: 'cron.async_reducer_handler',
  asyncProcedureHandler: 'cron.async_procedure_handler',
  invalidProcedureRecovery: 'cron.invalid_procedure_recovery',
  invalidJobName: 'cron.invalid_job_name',
  invalidHistoryCap: 'cron.invalid_history_cap',
  invalidReconcileInterval: 'cron.invalid_reconcile_interval',
} as const;
