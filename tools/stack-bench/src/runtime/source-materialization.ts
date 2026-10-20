import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';

import { redactCredentials } from '../evidence/diagnostic-sanitizer.js';
import { hashFiles } from '../evidence/provenance.js';
import type { RunOutcome } from '../evidence/outcomes.js';
import { controlAppServer } from './backend-control.js';
import type { RuntimeControlSpec } from './backend-control.js';
import { hashAppSource, resetAppToSource } from './source-snapshot.js';
import { CODING_CONTAINER_START_SCRIPT } from './coding-container-policy.js';
import { resetRepairBackend } from '../stacks/backend-reset.js';

const message = (error: unknown): string => error instanceof Error ? error.message : String(error);

// A rejected repair may have published a different schema. Restore code and
// recreate only its leased grading database before starting the accepted app.
// This boundary is deliberately separate from restart/durability probes.
export async function restoreRepairSource(sourcePath: string, appDir: string,
  application: RuntimeControlSpec,
  lifecycle: typeof controlAppServer = controlAppServer,
  reset: typeof resetRepairBackend = resetRepairBackend): Promise<void> {
  await materializeAcceptedSource(sourcePath, appDir, application, async (spec, mode, options) => {
    if (mode === 'start') reset({ backend: spec.backend, app: appDir });
    await lifecycle(spec, mode, options);
  });
}

export async function materializeAcceptedSource(sourcePath: string, appDir: string,
  application: RuntimeControlSpec,
  lifecycle: typeof controlAppServer = controlAppServer): Promise<void> {
  const accepted = hashAppSource(sourcePath);
  await lifecycle(application, 'stop');
  // A stopped app must start with only its saved inputs. Dependencies, logs and
  // other excluded files can carry rejected behavior too. The agent's own git
  // repository stays: later sessions use its history.
  resetAppToSource(sourcePath, appDir);
  if (!existsSync(join(appDir, 'start.sh'))) {
    throw Object.assign(new Error(`accepted application source has no ${CODING_CONTAINER_START_SCRIPT}`),
      { code: 'generated_app_start_contract_missing' });
  }
  let startFailure: unknown = null;
  try {
    await lifecycle(application, 'start', { cleanSource: true });
  } catch (error) {
    startFailure = error;
  }
  const restoreAcceptedSource = async (): Promise<void> => {
    let cleanupFailure: unknown = null;
    try {
      await lifecycle(application, 'stop');
    } catch (error) {
      cleanupFailure = error;
    }
    try {
      resetAppToSource(sourcePath, appDir);
    } catch (error) {
      cleanupFailure ??= error;
    }
    if (cleanupFailure) {
      throw new Error('could not stop and restore an application after startup',
        { cause: cleanupFailure });
    }
  };
  // Startup may add runtime state such as a build marker; it must not change or remove accepted files.
  const digest = (root: string, file: string) => hashFiles([join(root, file)], { base: root }).sha256;
  // A lock file decides what is installed. npm also copies the package's own name, version,
  // licence and declared ranges into it and rewrites them on a plain install after package.json
  // was edited by hand. With those apart, an identical lock installs the same packages; its
  // accepted bytes go back.
  const installs = (path: string): string | null => {
    try {
      const lock = JSON.parse(readFileSync(path, 'utf8'));
      const root = lock.packages?.[''] ?? {};
      for (const key of ['name', 'version']) delete lock[key];
      for (const key of ['name', 'version', 'license', 'dependencies', 'devDependencies',
        'optionalDependencies', 'peerDependencies']) delete root[key];
      return JSON.stringify(lock);
    } catch { return null; }
  };
  const sameInstall = (file: string): boolean => {
    if (basename(file) !== 'package-lock.json') return false;
    const started = installs(join(appDir, file));
    if (started === null || started !== installs(join(sourcePath, file))) return false;
    // Written in place: the file belongs to the app's user, and the controller may write it but not replace it.
    writeFileSync(join(appDir, file), readFileSync(join(sourcePath, file)));
    return true;
  };
  const changed = accepted.files.filter(file => !existsSync(join(appDir, file))
    || digest(appDir, file) !== digest(sourcePath, file) && !sameInstall(file));
  if (changed.length) {
    await restoreAcceptedSource();
    throw Object.assign(
      new Error(`materialized application source differs from its accepted snapshot: ${changed.slice(0, 5).join(', ')}`),
      { code: 'generated_app_source_changed' });
  }
  if (startFailure) {
    await restoreAcceptedSource();
    throw startFailure;
  }
}

export function materializationAppFailure(error: unknown): RunOutcome {
  const code = error && typeof error === 'object' && 'code' in error ? error.code : null;
  if (code !== 'generated_app_source_changed'
    && code !== 'generated_app_start_contract_missing'
    && code !== 'generated_app_not_restartable') throw error;
  const reason = code === 'generated_app_source_changed'
    ? `application startup changed the accepted source: ${message(error).split(': ').slice(1).join(': ')}`
    : code === 'generated_app_not_restartable'
    ? `application did not start from clean source: ${redactCredentials(message(error))
        .replace(/\s+/g, ' ').slice(0, 600)}`
    : `accepted application source has no ${CODING_CONTAINER_START_SCRIPT}`;
  return { kind: 'app_failure', phase: 'application-restart', reason,
    appFailures: ['application-restart'], inconclusive: [], harnessFailures: [] };
}
