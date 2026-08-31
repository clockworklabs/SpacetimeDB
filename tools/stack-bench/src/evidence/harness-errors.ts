// Only spawn and timeout failures are harness failures; command exits can describe app defects.
const INFRASTRUCTURE_CODES = new Set(['ETIMEDOUT', 'ENOENT', 'EACCES', 'EPERM', 'EPIPE']);
const BROWSER_INFRASTRUCTURE_FAILURE = Symbol('browserInfrastructureFailure');

interface ErrorDetails {
  readonly [BROWSER_INFRASTRUCTURE_FAILURE]?: unknown;
  readonly code?: unknown;
  readonly message?: unknown;
  readonly path?: unknown;
  readonly spawnfile?: unknown;
  readonly stderr?: unknown;
  readonly stdout?: unknown;
}

const errorDetails = (error: unknown): ErrorDetails =>
  error !== null && (typeof error === 'object' || typeof error === 'function')
    ? error as ErrorDetails
    : {};

export function harnessProcessFailure(error: unknown): string | null {
  if (!error) return null;
  const value = errorDetails(error);
  const detail = `${String(value.message ?? '')}\n${String(value.stderr ?? '')}\n${String(value.stdout ?? '')}`;
  if (/No such container:/i.test(detail)) {
    return 'database container selected by the harness is unavailable';
  }
  if (typeof value.code !== 'string' || !INFRASTRUCTURE_CODES.has(value.code)) return null;
  const command = String(value.path ?? value.spawnfile ?? 'child process');
  return `${command} failed in the harness (${value.code})`;
}

export function browserInfrastructureFailure(stage: string, error: unknown): Error {
  const message = errorDetails(error).message;
  const failure = new Error(`browser ${stage} failed: ${String(message ?? error)}`,
    { cause: error });
  Object.defineProperty(failure, BROWSER_INFRASTRUCTURE_FAILURE, { value: true });
  return failure;
}

export async function runBrowserInfrastructureOperation<Result>(
  stage: string,
  operation: () => Result | Promise<Result>,
): Promise<Result> {
  try { return await operation(); }
  catch (error) { throw browserInfrastructureFailure(stage, error); }
}

// Browser control failures are not proof that an application invariant failed.
// Navigation stays outside the explicit infrastructure wrapper, so ordinary
// application reachability failures keep their application-failure outcome.
export function harnessBrowserFailure(error: unknown): string | null {
  const value = errorDetails(error);
  const message = String(value.message ?? error ?? '');
  if (value[BROWSER_INFRASTRUCTURE_FAILURE]) return message;
  if (!/(?:Target|Page) crashed|Target page, context or browser has been closed/i.test(message)) return null;
  const firstLine = message.split(/\r?\n/, 1)[0] ?? '';
  return `browser target failed in the harness (${firstLine.slice(0, 200)})`;
}
