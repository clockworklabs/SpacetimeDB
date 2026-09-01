import { existsSync } from 'node:fs';
import { join } from 'node:path';

import { redactCredentials } from '../evidence/diagnostic-sanitizer.js';
import type { RunOutcome } from '../evidence/outcomes.js';
import { controlAppServer } from './backend-control.js';
import type { RuntimeControlSpec } from './backend-control.js';
import { hashAppSource, resetAppToSource } from './source-snapshot.js';
import { hashDirectory } from '../evidence/provenance.js';
import { CODING_CONTAINER_START_SCRIPT } from './coding-container-policy.js';

const message = (error: unknown): string => error instanceof Error ? error.message : String(error);

export async function materializeAcceptedSource(sourcePath: string, appDir: string,
  application: RuntimeControlSpec,
  lifecycle: typeof controlAppServer = controlAppServer): Promise<void> {
  const accepted = hashDirectory(sourcePath);
  await lifecycle(application, 'stop');
  resetAppToSource(sourcePath, appDir);
  if (!existsSync(join(appDir, 'start.sh'))) {
    throw Object.assign(new Error(`accepted application source has no ${CODING_CONTAINER_START_SCRIPT}`),
      { code: 'generated_app_start_contract_missing' });
  }
  let startFailure: unknown = null;
  try {
    await lifecycle(application, 'start');
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
  const materialized = hashAppSource(appDir);
  if (materialized.sha256 !== accepted.sha256
    || materialized.files.length !== accepted.files.length) {
    await restoreAcceptedSource();
    throw Object.assign(
      new Error('materialized application source differs from its accepted snapshot'),
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
    ? 'application startup changed the accepted source'
    : code === 'generated_app_not_restartable'
    ? `application did not start from clean source: ${redactCredentials(message(error))
        .replace(/\s+/g, ' ').slice(0, 600)}`
    : `accepted application source has no ${CODING_CONTAINER_START_SCRIPT}`;
  return { kind: 'app_failure', phase: 'application-restart', reason,
    appFailures: ['application-restart'], inconclusive: [], harnessFailures: [] };
}
