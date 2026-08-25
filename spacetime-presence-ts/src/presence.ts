import { Timestamp } from 'spacetimedb';

const ONE_SECOND_MICROS = 1_000_000n;
const U32_MAX = 0xffff_ffff;

export const DEFAULT_PRESENCE_TTL_SECONDS = 30;
export const DEFAULT_PRESENCE_SWEEP_BATCH = 500;
export const DEFAULT_PRESENCE_STATUS = 'online';
const MAX_SCOPE_LENGTH = 128;
const MAX_SUBJECT_LENGTH = 256;
const MAX_STATUS_LENGTH = 64;
const MAX_ACTIVITY_LENGTH = 256;
const MAX_PAYLOAD_LENGTH = 64 * 1024;
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

export interface PresenceSweepCtxLike extends PresenceTxLike {
  db: PresenceTxLike['db'] & {
    presenceConfig: {
      singleton: {
        find(key: boolean): PresenceConfigRow | null | undefined;
      };
    };
  };
}

export interface PresenceUpsertOpts {
  scope: string;
  subject: string;
  status?: string;
  activity?: string;
  payloadJson?: string;
  ttlSeconds?: number;
}

export interface PresenceInstallOpts {
  defaultTtlSeconds?: number;
  sweepBatch?: number;
}

function assertPositiveU32(name: string, value: number): void {
  if (!Number.isInteger(value) || value <= 0 || value > U32_MAX) {
    throw new Error(`presence.invalid_${name}`);
  }
}

function sanitize(name: string, value: string): string {
  const out = value.trim();
  if (out.length === 0) throw new Error(`presence.invalid_${name}`);
  const maxLength = name === 'scope' ? MAX_SCOPE_LENGTH : MAX_SUBJECT_LENGTH;
  if (out.length > maxLength) throw new Error(`presence.invalid_${name}`);
  return out;
}

function plusSeconds(ts: Timestamp, seconds: number): Timestamp {
  return new Timestamp(
    (ts.microsSinceUnixEpoch as bigint) + BigInt(seconds) * ONE_SECOND_MICROS
  );
}

export function buildPresenceKey(scope: string, subject: string): string {
  const normalizedScope = sanitize('scope', scope);
  const normalizedSubject = sanitize('subject', subject);
  return `${normalizedScope.length}:${normalizedScope}${normalizedSubject.length}:${normalizedSubject}`;
}

export function installPresenceConfig(
  ctx: PresenceConfigCtxLike,
  opts?: PresenceInstallOpts
): void {
  const defaultTtlSeconds =
    opts?.defaultTtlSeconds ?? DEFAULT_PRESENCE_TTL_SECONDS;
  const sweepBatch = opts?.sweepBatch ?? DEFAULT_PRESENCE_SWEEP_BATCH;
  assertPositiveU32('default_ttl_seconds', defaultTtlSeconds);
  assertPositiveU32('sweep_batch', sweepBatch);

  const existing = ctx.db.presenceConfig.singleton.find(true);
  if (!existing) {
    ctx.db.presenceConfig.insert({
      singleton: true,
      defaultTtlSeconds,
      sweepBatch,
      updatedAt: ctx.timestamp,
    });
    return;
  }
}

export function upsertPresence(
  tx: PresenceTxLike,
  opts: PresenceUpsertOpts
): PresenceEntryRow {
  const scope = sanitize('scope', opts.scope);
  const subject = sanitize('subject', opts.subject);
  const key = buildPresenceKey(scope, subject);
  const ttlSeconds = opts.ttlSeconds ?? DEFAULT_PRESENCE_TTL_SECONDS;
  assertPositiveU32('ttl_seconds', ttlSeconds);
  if (ttlSeconds > MAX_TTL_SECONDS)
    throw new Error('presence.invalid_ttl_seconds');

  const status = (opts.status ?? DEFAULT_PRESENCE_STATUS).trim();
  if (status.length === 0 || status.length > MAX_STATUS_LENGTH) {
    throw new Error('presence.invalid_status');
  }
  const activity = opts.activity?.trim() || undefined;
  if ((activity?.length ?? 0) > MAX_ACTIVITY_LENGTH) {
    throw new Error('presence.invalid_activity');
  }
  const payloadJson = opts.payloadJson?.trim() || undefined;
  if ((payloadJson?.length ?? 0) > MAX_PAYLOAD_LENGTH) {
    throw new Error('presence.invalid_payload');
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
    scope,
    subject,
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

export function touchPresence(
  tx: PresenceTxLike,
  scope: string,
  subject: string,
  ttlSeconds = DEFAULT_PRESENCE_TTL_SECONDS
): PresenceEntryRow {
  const key = buildPresenceKey(scope, subject);
  const existing = tx.db.presenceEntry.key.find(key);
  if (!existing) return upsertPresence(tx, { scope, subject, ttlSeconds });
  assertPositiveU32('ttl_seconds', ttlSeconds);
  if (ttlSeconds > MAX_TTL_SECONDS)
    throw new Error('presence.invalid_ttl_seconds');
  const now = tx.timestamp;
  const updated = {
    ...existing,
    lastSeenAt: now,
    expiresAt: plusSeconds(now, ttlSeconds),
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

export function sweepPresence(
  tx: PresenceTxLike,
  expiredRows: Iterable<PresenceEntryRow>,
  maxRows = DEFAULT_PRESENCE_SWEEP_BATCH
): number {
  assertPositiveU32('sweep_batch', maxRows);
  const nowMicros = tx.timestamp.microsSinceUnixEpoch as bigint;
  let deleted = 0;
  for (const row of expiredRows) {
    if (deleted >= maxRows) break;
    if ((row.expiresAt.microsSinceUnixEpoch as bigint) > nowMicros) break;
    tx.db.presenceEntry.delete(row);
    deleted++;
  }
  return deleted;
}

export function resolvePresenceSweepBatch(
  ctx: PresenceSweepCtxLike,
  fallback = DEFAULT_PRESENCE_SWEEP_BATCH
): number {
  const cfg = ctx.db.presenceConfig.singleton.find(true);
  const value = Number(cfg?.sweepBatch ?? fallback);
  return value > 0 ? value : fallback;
}

export function runPresenceSweep(
  ctx: PresenceSweepCtxLike,
  expiredRows: Iterable<PresenceEntryRow>
): number {
  const batch = resolvePresenceSweepBatch(ctx);
  return sweepPresence(ctx, expiredRows, batch);
}
