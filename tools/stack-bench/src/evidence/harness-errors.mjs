// Errors raised by the harness's own child processes are not evidence that the
// application under test is wrong. Keep this deliberately narrow: a command
// which exits non-zero can still be reporting an application/schema defect,
// while failure to spawn or a forced process timeout means the measurement did
// not complete.
const INFRASTRUCTURE_CODES = new Set(['ETIMEDOUT', 'ENOENT', 'EACCES', 'EPERM', 'EPIPE']);
const BROWSER_INFRASTRUCTURE_FAILURE = Symbol('browserInfrastructureFailure');

export function harnessProcessFailure(error) {
  if (!error) return null;
  const detail = `${error.message ?? ''}\n${error.stderr ?? ''}\n${error.stdout ?? ''}`;
  if (/No such container:/i.test(detail)) {
    return 'database container selected by the harness is unavailable';
  }
  if (!INFRASTRUCTURE_CODES.has(error.code)) return null;
  const command = error.path ?? error.spawnfile ?? 'child process';
  return `${command} failed in the harness (${error.code})`;
}

export function browserInfrastructureFailure(stage, error) {
  const failure = new Error(`browser ${stage} failed: ${error?.message ?? String(error)}`,
    { cause: error });
  Object.defineProperty(failure, BROWSER_INFRASTRUCTURE_FAILURE, { value: true });
  return failure;
}

export async function runBrowserInfrastructureOperation(stage, operation) {
  try { return await operation(); }
  catch (error) { throw browserInfrastructureFailure(stage, error); }
}

// Browser control failures are not proof that an application invariant failed.
// Navigation stays outside the explicit infrastructure wrapper, so ordinary
// application reachability failures keep their application-failure outcome.
export function harnessBrowserFailure(error) {
  const message = String(error?.message ?? error ?? '');
  if (error?.[BROWSER_INFRASTRUCTURE_FAILURE]) return message;
  if (!/(?:Target|Page) crashed|Target page, context or browser has been closed/i.test(message)) return null;
  return `browser target failed in the harness (${message.split(/\r?\n/)[0].slice(0, 200)})`;
}
