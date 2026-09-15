import { existsSync, mkdirSync, readFileSync, realpathSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';
import { createHash } from 'node:crypto';
import { z } from 'zod';
import { leaseFromEnv } from './backend-lease.js';
import type { BackendLease } from './backend-lease.js';
import { handoffBuildWorkspace } from './backend-teardown.js';
import { inspectBuildContainer, sameHostPath } from '../../container/build-container-inspection.js';
import { checkpointDatabaseTarget, checkpointFileHash, copyCheckpointDatabase,
  startCheckpointDatabase, stopCheckpointDatabase } from '../stacks/database-checkpoint.js';

import { redactCredentials } from '../evidence/diagnostic-sanitizer.js';
import type { RunOutcome } from '../evidence/outcomes.js';
import { controlAppServer } from './backend-control.js';
import type { RuntimeControlSpec } from './backend-control.js';
import { hashAppSource, snapshotAppSource, restoreAppSource } from './source-snapshot.js';
import { CODING_CONTAINER_APP_ROOT, CODING_CONTAINER_START_SCRIPT } from './coding-container-policy.js';
import { resetRepairBackend } from '../stacks/backend-reset.js';

const message = (error: unknown): string => error instanceof Error ? error.message : String(error);

// A source path and a database lease are not enough: the running application
// must use that exact source tree. Refuse a mixed pair before any lifecycle call.
export function requirePopulatedWorkspace(lease: { resources: Pick<BackendLease['resources'], 'buildContainer'> }, app: string,
  inspect: typeof inspectBuildContainer = inspectBuildContainer) {
  const recorded = lease.resources.buildContainer;
  if (!recorded?.owned) throw new Error('populated task requires an owned build container');
  const actual = inspect(recorded.name);
  const mounts = actual?.mounts.filter(mount => mount.destination === CODING_CONTAINER_APP_ROOT) ?? [];
  if (!actual?.running || actual.id !== recorded.id || mounts.length !== 1
    || mounts[0]!.type !== 'bind' || mounts[0]!.readOnly || !sameHostPath(mounts[0]!.source, app)) {
    throw new Error('populated task source does not match its leased build workspace');
  }
  return actual;
}

// A rejected repair may have published a different schema. Restore code and
// recreate only its leased grading database before starting the accepted app.
// This boundary is deliberately separate from restart/durability probes.
export async function restoreRepairSource(sourcePath: string, appDir: string,
  application: RuntimeControlSpec,
  lifecycle: typeof controlAppServer = controlAppServer,
  reset: typeof resetRepairBackend = resetRepairBackend,
  populatedCheckpoint?: string): Promise<void> {
  if (populatedCheckpoint) {
    if (resolve(appDir) !== resolve(application.app)) throw new Error('repair application paths do not match');
    await restorePopulatedCheckpoint(populatedCheckpoint, application, sourcePath);
    return;
  }
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
  restoreAppSource(sourcePath, appDir);
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
      restoreAppSource(sourcePath, appDir);
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

const hash = z.string().regex(/^[a-f0-9]{64}$/);
const receiptSchema = z.object({
  version: z.literal(1), backend: z.enum(['postgres', 'mongodb', 'spacetime']),
  runId: z.string().min(1), ownershipSha256: hash, container: z.string().min(1),
  image: z.string().min(1), app: z.string().min(1), createdAt: z.string().datetime(),
  sourceSha256: hash, dataSha256: hash, dataBytes: z.number().int().positive(),
}).strict();
export type PopulatedCheckpoint = z.infer<typeof receiptSchema>;

const ownershipHash = (lease: BackendLease) => createHash('sha256').update(lease.ownershipToken).digest('hex');
const inside = (parent: string, child: string) => {
  const path = relative(parent, child);
  return path === '' || (path !== '..' && !path.startsWith(`..${sep}`) && !isAbsolute(path));
};

function checkpointLocation(directory: string, app: string, creating = false): string {
  const target = resolve(directory);
  const realApp = realpathSync(app);
  // Resolve parent links before creating anything. A private checkpoint must
  // never be in (or contain) a directory exposed to the coding agent.
  const realTarget = creating ? join(realpathSync(dirname(target)), basename(target))
    : realpathSync(target);
  if (realTarget !== target || inside(realApp, target) || inside(target, realApp)) {
    throw new Error('populated checkpoint must be a separate private directory without path links');
  }
  return target;
}

export async function validatePopulatedCheckpoint(directory: string, application: RuntimeControlSpec,
  lease: BackendLease): Promise<PopulatedCheckpoint> {
  const target = checkpointLocation(directory, application.app);
  const receipt = receiptSchema.parse(JSON.parse(readFileSync(join(target, 'checkpoint.json'), 'utf8')));
  if (receipt.backend !== application.backend || receipt.backend !== lease.backend
    || receipt.runId !== lease.runId || receipt.ownershipSha256 !== ownershipHash(lease)
    || receipt.container !== lease.resources.container?.id || receipt.image !== lease.resources.container?.image
    || receipt.app !== realpathSync(application.app)) {
    throw new Error('populated checkpoint does not belong to this application and lease');
  }
  if (hashAppSource(join(target, 'source')).sha256 !== receipt.sourceSha256
    || statSync(join(target, 'database.tar')).size !== receipt.dataBytes
    || await checkpointFileHash(join(target, 'database.tar')) !== receipt.dataSha256) {
    throw new Error('populated checkpoint source or database archive changed');
  }
  return receipt;
}

export async function capturePopulatedCheckpoint(directory: string,
  application: RuntimeControlSpec): Promise<PopulatedCheckpoint> {
  const target = checkpointLocation(directory, application.app, true);
  const { lease } = leaseFromEnv(process.env, { backend: application.backend, active: true });
  const database = checkpointDatabaseTarget(lease);
  const build = requirePopulatedWorkspace(lease, application.app);
  if (!lease.resources.container?.image) throw new Error('database checkpoint requires a recorded image');
  mkdirSync(target, { mode: 0o700 });
  await controlAppServer(application, 'stop');
  handoffBuildWorkspace(build.id);
  stopCheckpointDatabase(lease);
  // Any failure stops the attempt; never restart a partially captured/restored
  // database and pretend that the next candidate has a known starting state.
  snapshotAppSource(application.app, join(target, 'source'));
  const source = hashAppSource(application.app);
  if (hashAppSource(join(target, 'source')).sha256 !== source.sha256) {
    throw new Error('source changed during populated checkpoint capture');
  }
  const archive = join(target, 'database.tar');
  copyCheckpointDatabase(lease, archive, false);
  const receipt = receiptSchema.parse({ version: 1, backend: lease.backend, runId: lease.runId,
    ownershipSha256: ownershipHash(lease), container: database.container, image: lease.resources.container.image,
    app: realpathSync(application.app), createdAt: new Date().toISOString(), sourceSha256: source.sha256,
    dataSha256: await checkpointFileHash(archive), dataBytes: statSync(archive).size });
  writeFileSync(join(target, 'checkpoint.json'), JSON.stringify(receipt, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  await startCheckpointDatabase(lease);
  await materializeAcceptedSource(join(target, 'source'), application.app, application);
  return receipt;
}

export async function restorePopulatedCheckpoint(directory: string,
  application: RuntimeControlSpec, acceptedSource?: string): Promise<PopulatedCheckpoint> {
  const { lease } = leaseFromEnv(process.env, { backend: application.backend, active: true });
  // Authenticate and hash before stopping or changing any live state.
  const receipt = await validatePopulatedCheckpoint(directory, application, lease);
  if (acceptedSource && hashAppSource(acceptedSource).sha256 !== receipt.sourceSha256) {
    throw new Error('repair source does not match its populated database checkpoint');
  }
  checkpointDatabaseTarget(lease);
  const build = requirePopulatedWorkspace(lease, application.app);
  await controlAppServer(application, 'stop');
  handoffBuildWorkspace(build.id);
  stopCheckpointDatabase(lease);
  restoreAppSource(join(directory, 'source'), application.app);
  copyCheckpointDatabase(lease, join(directory, 'database.tar'), true);
  await startCheckpointDatabase(lease);
  await materializeAcceptedSource(join(directory, 'source'), application.app, application);
  return receipt;
}
