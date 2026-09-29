import { SenderError, Timestamp } from 'spacetimedb';
import { errors } from './errors.js';

const ONE_SECOND_MICROS = 1_000_000n;
const U32_MAX = 0xffff_ffff;

export const DEFAULT_PRESENCE_TTL_SECONDS = 30;
export const DEFAULT_PRESENCE_SWEEP_BATCH = 500;
export const MAX_PRESENCE_SWEEP_BATCH = 10_000;
export const DEFAULT_PRESENCE_STATUS = 'online';
const MAX_SCOPE_LENGTH = 128;
const MAX_SUBJECT_LENGTH = 256;
const MAX_STATUS_LENGTH = 64;
const MAX_ACTIVITY_LENGTH = 256;
const MAX_PAYLOAD_LENGTH = 4096;
const MAX_TTL_SECONDS = 3600;

export interface PresenceEntryRow {
  key: string;
  scope: string;
  subject: string;
  status: string;
  activity: string | undefined;
  payloadJson: string | undefined;
  joinedAt: Timestamp;
  lastSeenAt: Timestamp;
  expiresAt: Timestamp;
  updatedAt: Timestamp;
}

interface PresenceConfigRow {
  singleton: boolean;
  defaultTtlSeconds: number;
  sweepBatch: number;
  updatedAt: Timestamp;
}

/** A transaction over the presence entry and config tables. */
export interface PresenceTxLike {
  timestamp: Timestamp;
  db: {
    presenceEntry: {
      key: {
        find(key: string): PresenceEntryRow | null | undefined;
        update(row: PresenceEntryRow): void;
      };
      insert(row: PresenceEntryRow): void;
      delete(row: PresenceEntryRow): void;
    };
    presenceConfig: {
      singleton: {
        find(key: boolean): PresenceConfigRow | null | undefined;
      };
    };
  };
}

export interface PresenceConfigCtxLike {
  timestamp: Timestamp;
  db: {
    presenceConfig: {
      singleton: {
        find(key: boolean): PresenceConfigRow | null | undefined;
        update(row: PresenceConfigRow): void;
      };
      insert(row: PresenceConfigRow): void;
    };
  };
}

export interface PresenceUpsertOpts {
  scope: string;
  subject: string;
  status?: string;
  activity?: string;
  payloadJson?: string;
  /** Defaults to the configured `defaultTtlSeconds`. */
  ttlSeconds?: number;
}

export interface PresenceInstallOpts {
  defaultTtlSeconds?: number;
  sweepBatch?: number;
}

function assertTtl(ttlSeconds: number): void {
  if (
    !Number.isInteger(ttlSeconds) ||
    ttlSeconds <= 0 ||
    ttlSeconds > MAX_TTL_SECONDS
  ) {
    throw new SenderError(errors.invalidTtlSeconds);
  }
}

function assertSweepBatch(value: number): void {
  if (
    !Number.isInteger(value) ||
    value <= 0 ||
    value > MAX_PRESENCE_SWEEP_BATCH
  ) {
    throw new SenderError(errors.invalidSweepBatch);
  }
}

function sanitize(code: string, value: string, maxLength: number): string {
  const out = value.trim();
  if (out.length === 0 || out.length > maxLength) throw new SenderError(code);
  return out;
}

function plusSeconds(ts: Timestamp, seconds: number): Timestamp {
  return new Timestamp(
    ts.microsSinceUnixEpoch + BigInt(seconds) * ONE_SECOND_MICROS
  );
}

function defaultTtl(tx: PresenceTxLike): number {
  return (
    tx.db.presenceConfig.singleton.find(true)?.defaultTtlSeconds ??
    DEFAULT_PRESENCE_TTL_SECONDS
  );
}

export function buildPresenceKey(scope: string, subject: string): string {
  const normalizedScope = sanitize(
    errors.invalidScope,
    scope,
    MAX_SCOPE_LENGTH
  );
  const normalizedSubject = sanitize(
    errors.invalidSubject,
    subject,
    MAX_SUBJECT_LENGTH
  );
  return `${normalizedScope.length}:${normalizedScope}${normalizedSubject.length}:${normalizedSubject}`;
}

/** Inserts the config row once. Later calls keep the stored config. */
export function installPresenceConfig(
  ctx: PresenceConfigCtxLike,
  opts?: PresenceInstallOpts
): void {
  const defaultTtlSeconds =
    opts?.defaultTtlSeconds ?? DEFAULT_PRESENCE_TTL_SECONDS;
  const sweepBatch = opts?.sweepBatch ?? DEFAULT_PRESENCE_SWEEP_BATCH;
  assertTtl(defaultTtlSeconds);
  assertSweepBatch(sweepBatch);
  if (ctx.db.presenceConfig.singleton.find(true)) return;
  ctx.db.presenceConfig.insert({
    singleton: true,
    defaultTtlSeconds,
    sweepBatch,
    updatedAt: ctx.timestamp,
  });
}

export function updatePresenceConfig(
  ctx: PresenceConfigCtxLike,
  opts: Required<PresenceInstallOpts>
): void {
  assertTtl(opts.defaultTtlSeconds);
  assertSweepBatch(opts.sweepBatch);
  const existing = ctx.db.presenceConfig.singleton.find(true);
  if (!existing) throw new SenderError(errors.configMissing);
  ctx.db.presenceConfig.singleton.update({
    ...existing,
    defaultTtlSeconds: opts.defaultTtlSeconds,
    sweepBatch: opts.sweepBatch,
    updatedAt: ctx.timestamp,
  });
}

export function upsertPresence(
  tx: PresenceTxLike,
  opts: PresenceUpsertOpts
): PresenceEntryRow {
  const scope = sanitize(errors.invalidScope, opts.scope, MAX_SCOPE_LENGTH);
  const subject = sanitize(
    errors.invalidSubject,
    opts.subject,
    MAX_SUBJECT_LENGTH
  );
  const key = buildPresenceKey(scope, subject);
  const ttlSeconds = opts.ttlSeconds ?? defaultTtl(tx);
  assertTtl(ttlSeconds);

  const status = sanitize(
    errors.invalidStatus,
    opts.status ?? DEFAULT_PRESENCE_STATUS,
    MAX_STATUS_LENGTH
  );
  const activity = opts.activity?.trim() || undefined;
  if ((activity?.length ?? 0) > MAX_ACTIVITY_LENGTH) {
    throw new SenderError(errors.invalidActivity);
  }
  const payloadJson = opts.payloadJson?.trim() || undefined;
  if ((payloadJson?.length ?? 0) > MAX_PAYLOAD_LENGTH) {
    throw new SenderError(errors.invalidPayload);
  }

  const now = tx.timestamp;
  const expiresAt = plusSeconds(now, ttlSeconds);
  const existing = tx.db.presenceEntry.key.find(key);

  if (!existing) {
    const inserted: PresenceEntryRow = {
      key,
      scope,
      subject,
      status,
      activity,
      payloadJson,
      joinedAt: now,
      lastSeenAt: now,
      expiresAt,
      updatedAt: now,
    };
    tx.db.presenceEntry.insert(inserted);
    return inserted;
  }

  const updated: PresenceEntryRow = {
    ...existing,
    status,
    activity,
    payloadJson,
    lastSeenAt: now,
    expiresAt,
    updatedAt: now,
  };
  tx.db.presenceEntry.key.update(updated);
  return updated;
}

/** Extends a lease and keeps its status, activity, and payload. */
export function touchPresence(
  tx: PresenceTxLike,
  scope: string,
  subject: string,
  ttlSeconds?: number
): PresenceEntryRow {
  const key = buildPresenceKey(scope, subject);
  const existing = tx.db.presenceEntry.key.find(key);
  if (!existing) return upsertPresence(tx, { scope, subject, ttlSeconds });
  const ttl = ttlSeconds ?? defaultTtl(tx);
  assertTtl(ttl);
  const now = tx.timestamp;
  const updated = {
    ...existing,
    lastSeenAt: now,
    expiresAt: plusSeconds(now, ttl),
    updatedAt: now,
  };
  tx.db.presenceEntry.key.update(updated);
  return updated;
}

export function removePresence(
  tx: PresenceTxLike,
  scope: string,
  subject: string
): boolean {
  const key = buildPresenceKey(scope, subject);
  const existing = tx.db.presenceEntry.key.find(key);
  if (!existing) return false;
  tx.db.presenceEntry.delete(existing);
  return true;
}

/**
 * Deletes up to the configured sweep batch of expired rows from `rows`,
 * usually the `expiresAt` index filtered to rows at or before now. Unexpired
 * rows are skipped, so any iterable is safe.
 */
export function runPresenceSweep(
  tx: PresenceTxLike,
  rows: Iterable<PresenceEntryRow>
): number {
  const batch =
    tx.db.presenceConfig.singleton.find(true)?.sweepBatch ??
    DEFAULT_PRESENCE_SWEEP_BATCH;
  const now = tx.timestamp.microsSinceUnixEpoch;
  const expired: PresenceEntryRow[] = [];
  for (const row of rows) {
    if (expired.length >= batch) break;
    if (row.expiresAt.microsSinceUnixEpoch <= now) expired.push(row);
  }
  for (const row of expired) tx.db.presenceEntry.delete(row);
  return expired.length;
}
