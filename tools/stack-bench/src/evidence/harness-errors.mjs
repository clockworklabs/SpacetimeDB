// Errors raised by the harness's own child processes are not evidence that the
// application under test is wrong. Keep this deliberately narrow: a command
// which exits non-zero can still be reporting an application/schema defect,
// while failure to spawn or a forced process timeout means the measurement did
// not complete.
const INFRASTRUCTURE_CODES = new Set(['ETIMEDOUT', 'ENOENT', 'EACCES', 'EPERM', 'EPIPE']);

export function harnessProcessFailure(error) {
  if (!error || !INFRASTRUCTURE_CODES.has(error.code)) return null;
  const command = error.path ?? error.spawnfile ?? 'child process';
  return `${command} failed in the harness (${error.code})`;
}

// A browser target disappearing while Playwright is issuing a harness control
// operation is not proof that an application invariant failed. The application
// may have contributed to renderer pressure, but a crashed target supplies no
// behavioral observation; preserve it as inconclusive infrastructure evidence.
export function harnessBrowserFailure(error) {
  const message = String(error?.message ?? error ?? '');
  if (!/(?:Target|Page) crashed|Target page, context or browser has been closed/i.test(message)) return null;
  return `browser target failed in the harness (${message.split(/\r?\n/)[0].slice(0, 200)})`;
}
