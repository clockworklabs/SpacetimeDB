import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve } from 'node:path';
import { z } from 'zod';
import { readArtifact } from '../evidence/artifacts.js';
import { hashAppSource } from './source-snapshot.js';

const sha = z.string().regex(/^[a-f0-9]{64}$/);
export const savedDiagnosticSchema = z.strictObject({
  run: z.string().min(1), runSha256: sha, checkpoint: z.number().int().positive(),
  source: z.string().min(1), reader: z.strictObject({ path: z.string().min(1), sha256: sha }),
});
const digest = (path: string) => createHash('sha256').update(readFileSync(path)).digest('hex');
const backend = z.enum(['postgres', 'mongodb', 'spacetime']);
const runSchema = z.object({
  backend, track: z.literal('ecommerce'), contaminated: z.literal(false),
  runtime: z.object({ buildImage: z.string().regex(/^sha256:[a-f0-9]{64}$/) }),
  backendLease: z.object({ runIndex: z.number().int().nonnegative(), resources: z.object({
    serverUri: z.string().nullable(), module: z.string().nullable(), database: z.string().nullable(),
    container: z.object({ image: z.string().regex(/^sha256:[a-f0-9]{64}$/) }).optional(),
  }) }),
  checkpoints: z.array(z.object({ sequence: z.number(), level: z.number(), accepted: z.boolean(), excluded: z.boolean().optional(),
    sourceSha256: sha, selectionSha256: sha, evidence: z.object({ path: z.string(), sha256: sha }) })),
  condition: z.object({ guidance: z.object({ credentialAliases: z.record(z.string(), z.unknown()).optional() }) }),
});

// Explicit accepted-checkpoint replay for zero-point diagnostics. Normal regrade
// still selects first builds and never accepts this independent audit reader.
export function inspectSavedDiagnostic(input: unknown, base: string) {
  const request = savedDiagnosticSchema.parse(input);
  const runPath = resolve(base, request.run), source = resolve(base, request.source);
  const reader = { ...request.reader, path: resolve(base, request.reader.path) };
  if (digest(runPath) !== request.runSha256) throw new Error('saved diagnostic run changed');
  const artifact = readArtifact(runPath, { expectedKind: 'benchmark_run' });
  if (!artifact.timestamps.completedAt) throw new Error('saved diagnostic requires a completed run');
  const run = runSchema.parse(artifact.payload);
  const { serverUri, module, database } = run.backendLease.resources;
  if (run.backend === 'spacetime') {
    if (!run.backendLease.resources.container) throw new Error('saved SpacetimeDB requires its original backend image');
    const uri = new URL(serverUri ?? '');
    if (uri.protocol !== 'http:' || uri.hostname !== '127.0.0.1' || !uri.port || uri.username || uri.password
      || uri.pathname !== '/' || uri.search || uri.hash || !module) throw new Error('saved diagnostic requires a local SpacetimeDB target');
  } else if (!database) throw new Error('saved diagnostic requires the original database name');
  if (new Set(run.checkpoints.map(row => row.sequence)).size !== run.checkpoints.length) {
    throw new Error('saved diagnostic checkpoint sequences are ambiguous');
  }
  const declaration = run.checkpoints.find(row => row.sequence === request.checkpoint);
  if (!declaration?.accepted || declaration.excluded || declaration.level !== 3
    || run.checkpoints.some(row => row.accepted && row.sequence > declaration.sequence)) {
    throw new Error('saved diagnostic requires the final accepted L3 checkpoint');
  }
  const checkpointPath = resolve(dirname(runPath), declaration.evidence.path);
  const pathFromRun = relative(dirname(runPath), checkpointPath);
  if (isAbsolute(pathFromRun) || pathFromRun.startsWith('..') || digest(checkpointPath) !== declaration.evidence.sha256) {
    throw new Error('saved diagnostic checkpoint changed or escaped its run');
  }
  const checkpoint = z.object({ backend, track: z.literal('ecommerce'), level: z.literal(3),
    source: z.object({ sha256: sha }), selection: z.object({ sha256: sha,
      checks: z.array(z.object({ stableKey: z.string() })).nonempty() })
  }).parse(readArtifact(checkpointPath, { expectedKind: 'grade_bundle' }).payload);
  if (checkpoint.backend !== run.backend || checkpoint.source.sha256 !== declaration.sourceSha256
    || checkpoint.selection.sha256 !== declaration.selectionSha256
    || hashAppSource(source).sha256 !== declaration.sourceSha256) throw new Error('saved diagnostic source or selection mismatch');
  const keys = checkpoint.selection.checks.map(row => row.stableKey);
  if (new Set(keys).size !== keys.length || keys.some(key => /payment-records|payment-deduplication|reservation/.test(key))) {
    throw new Error('saved order reader requires an order-only selection without reservations');
  }
  if (digest(reader.path) !== reader.sha256
    || JSON.parse(readFileSync(reader.path, 'utf8')).sourceSha256 !== declaration.sourceSha256) {
    throw new Error('saved diagnostic reader does not match its accepted source');
  }
  return { ...request, run: runPath, source, reader, sourceSha256: declaration.sourceSha256,
    selectionSha256: declaration.selectionSha256, backend: run.backend, track: run.track,
    runIndex: run.backendLease.runIndex, buildImage: run.runtime.buildImage,
    serverUri, module, database, backendImage: run.backendLease.resources.container?.image ?? null,
    credentialAliases: run.condition.guidance.credentialAliases ?? {} };
}
