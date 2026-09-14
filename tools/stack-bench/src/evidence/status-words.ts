// The words a person reads for a machine token. Artifacts keep the token;
// every surface that prints for a human (CLI status, console, dashboard)
// goes through here, so one state has one name everywhere.
const STATUS_WORD: Readonly<Record<string, string>> = Object.freeze({
  prepared: 'ready',
  pending: 'queued',
  'attention-required': 'needs attention',
  invalid: 'excluded',
  working: 'feature checks passed; guarantees incomplete',
  app_failure: 'application failure',
  harness_failure: 'harness failure',
  provider_failure: 'provider failure',
  ungraded: 'not graded',
  timed_out: 'timed out',
  missing_artifact: 'no run record',
  scheduler_interrupted: 'scheduler interrupted',
});

export function statusWord(status: string): string {
  return STATUS_WORD[status] ?? status;
}
