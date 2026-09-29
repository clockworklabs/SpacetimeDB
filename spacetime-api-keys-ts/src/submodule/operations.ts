import { Timestamp } from 'spacetimedb';
import { Range, SenderError, type Infer } from 'spacetimedb/server';
import {
  ApiKeyStatus,
  apiKey,
  apiKeyCreateResult,
  apiKeySummary,
  apiKeySweepTick,
  apiKeyUsageSummary,
  spacetimedb,
  t,
  type ViewModuleCtx,
  type WriteCtx,
} from './schema.js';
import { isAdmin, requireAdmin } from './auth.js';
import { errors } from '../errors.js';
import {
  deriveKeySecret,
  extractLookupPrefix,
  formatApiKey,
  hashApiKey,
  matchesApiKeyHash,
  hasScope,
} from '../keys.js';

const DEFAULT_KEY_PREFIX = 'stdb_live';
const MAX_NAME_LENGTH = 120;
const MAX_OWNER_SUBJECT_LENGTH = 256;
const MAX_SCOPE_LENGTH = 128;
const MAX_SCOPES = 128;
const MAX_METADATA_JSON_LENGTH = 8192;
const MAX_RAW_KEY_LENGTH = 128;
const MAX_ACTIVE_KEYS_PER_OWNER = 50;
const MAX_EXPIRATION_SECONDS = 60 * 60 * 24 * 365 * 10;
const MIN_SECRET_LENGTH = 32;
const MIN_USAGE_RETENTION_SECONDS = 60 * 60;
const DEFAULT_USAGE_RETENTION_SECONDS = 60 * 60 * 24 * 30;
const USAGE_SWEEP_BATCH = 1000;
const ONE_SECOND_MICROS = 1_000_000n;

type ApiKeyRow = Infer<typeof apiKey.rowType>;

export type CreateApiKeyArgs = {
  ownerSubject: string;
  name: string;
  scopesJson: string;
  metadataJson?: string | undefined;
  expiresInSeconds?: number | undefined;
  keyPrefix?: string | undefined;
};

export type VerifyApiKeyArgs = {
  key: string;
  requiredScope?: string | undefined;
  action?: string | undefined;
};

export type ApiKeyVerifyResult = {
  allowed: boolean;
  reason: string;
  keyId: string | undefined;
  prefix: string | undefined;
  ownerSubject: string | undefined;
  scopesJson: string | undefined;
  metadataJson: string | undefined;
};

function throwSenderError(message: string): never {
  throw new SenderError(message);
}

function takeRows<T>(rows: Iterable<T>, limit: number): T[] {
  const out: T[] = [];
  for (const row of rows) {
    if (out.length >= limit) break;
    out.push(row);
  }
  return out;
}

function generateRawKey(ctx: WriteCtx, keyPrefix: string) {
  const cfg = ctx.db.apiKeyConfig.singleton.find(true);
  if (!cfg) throwSenderError(errors.configMissing);
  const counter = cfg.counter + 1n;
  ctx.db.apiKeyConfig.singleton.update({ ...cfg, counter });
  return formatApiKey(
    keyPrefix,
    deriveKeySecret(cfg.secret, counter, ctx.timestamp.microsSinceUnixEpoch)
  );
}

function normalizeKeyPrefix(prefix: string | undefined): string {
  const value = (prefix ?? DEFAULT_KEY_PREFIX).trim();
  if (!/^[A-Za-z][A-Za-z0-9_]{1,31}$/.test(value)) {
    throwSenderError(errors.invalidKeyPrefix);
  }
  return value;
}

function normalizeName(name: string): string {
  const value = name.trim().replace(/\s+/g, ' ');
  if (value.length === 0 || value.length > MAX_NAME_LENGTH) {
    throwSenderError(errors.invalidName);
  }
  return value;
}

function normalizeOwnerSubject(ownerSubject: string): string {
  const value = ownerSubject.trim();
  if (value.length === 0 || value.length > MAX_OWNER_SUBJECT_LENGTH) {
    throwSenderError(errors.invalidOwnerSubject);
  }
  return value;
}

function normalizeAction(
  action: string | undefined,
  requiredScope: string | undefined
): string {
  const value = (action ?? requiredScope ?? 'verify').trim();
  if (value.length === 0 || value.length > MAX_SCOPE_LENGTH) {
    throwSenderError(errors.invalidAction);
  }
  return value;
}

function normalizeRequiredScope(
  requiredScope: string | undefined
): string | undefined {
  if (requiredScope === undefined) return undefined;
  const value = requiredScope.trim();
  if (
    value.length === 0 ||
    value.length > MAX_SCOPE_LENGTH ||
    !/^[A-Za-z0-9:_*.-]+$/.test(value)
  ) {
    throwSenderError(errors.invalidRequiredScope);
  }
  return value;
}

function normalizeScopesJson(scopesJson: string): string {
  let parsed: unknown;
  try {
    parsed = JSON.parse(scopesJson);
  } catch {
    throwSenderError(errors.invalidScopesJson);
  }
  if (!Array.isArray(parsed)) throwSenderError(errors.invalidScopesJson);
  if (parsed.length === 0 || parsed.length > MAX_SCOPES) {
    throwSenderError(errors.invalidScopesJson);
  }
  const seen = new Set<string>();
  const scopes: string[] = [];
  for (const raw of parsed) {
    if (typeof raw !== 'string') throwSenderError(errors.invalidScopesJson);
    const scope = raw.trim();
    if (scope.length === 0 || scope.length > MAX_SCOPE_LENGTH) {
      throwSenderError(errors.invalidScopesJson);
    }
    if (!/^[A-Za-z0-9:_*.-]+$/.test(scope)) {
      throwSenderError(errors.invalidScopesJson);
    }
    if (!seen.has(scope)) {
      seen.add(scope);
      scopes.push(scope);
    }
  }
  return JSON.stringify(scopes);
}

function normalizeMetadataJson(
  metadataJson: string | undefined
): string | undefined {
  if (metadataJson === undefined) return undefined;
  const value = metadataJson.trim();
  if (value.length === 0) return undefined;
  if (value.length > MAX_METADATA_JSON_LENGTH)
    throwSenderError(errors.invalidMetadataJson);
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    throwSenderError(errors.invalidMetadataJson);
  }
  if (parsed === null || Array.isArray(parsed) || typeof parsed !== 'object') {
    throwSenderError(errors.invalidMetadataJson);
  }
  return JSON.stringify(parsed);
}

function expiresAtFromSeconds(
  ctx: WriteCtx,
  expiresInSeconds: number | undefined
): Timestamp | undefined {
  if (expiresInSeconds === undefined) return undefined;
  if (
    !Number.isInteger(expiresInSeconds) ||
    expiresInSeconds <= 0 ||
    expiresInSeconds > MAX_EXPIRATION_SECONDS
  ) {
    throwSenderError(errors.invalidExpiresInSeconds);
  }
  return new Timestamp(
    ctx.timestamp.microsSinceUnixEpoch +
      BigInt(expiresInSeconds) * ONE_SECOND_MICROS
  );
}

function isExpired(row: ApiKeyRow, now: Timestamp): boolean {
  return (
    row.expiresAt !== undefined &&
    row.expiresAt.microsSinceUnixEpoch <= now.microsSinceUnixEpoch
  );
}

function toSummary(row: ApiKeyRow) {
  return {
    keyId: row.keyId,
    prefix: row.prefix,
    ownerSubject: row.ownerSubject,
    name: row.name,
    scopesJson: row.scopesJson,
    metadataJson: row.metadataJson,
    status: row.status,
    createdAt: row.createdAt,
    expiresAt: row.expiresAt,
    lastUsedAt: row.lastUsedAt,
    revokedAt: row.revokedAt,
  };
}

function toCreateResult(row: ApiKeyRow, key: string) {
  return {
    keyId: row.keyId,
    key,
    prefix: row.prefix,
    ownerSubject: row.ownerSubject,
    name: row.name,
    scopesJson: row.scopesJson,
    metadataJson: row.metadataJson,
    status: row.status,
    createdAt: row.createdAt,
    expiresAt: row.expiresAt,
  };
}

function recordUsage(
  ctx: WriteCtx,
  args: {
    keyId?: string | undefined;
    prefix?: string | undefined;
    ownerSubject?: string | undefined;
    action: string;
    allowed: boolean;
    reason: string;
  }
): void {
  ctx.db.apiKeyUsage.insert({
    usageId: 0n,
    keyId: args.keyId ?? '',
    prefix: args.prefix ?? '',
    ownerSubject: args.ownerSubject ?? '',
    action: args.action,
    allowed: args.allowed,
    reason: args.reason,
    usedAt: ctx.timestamp,
    usedAtOrder: -ctx.timestamp.microsSinceUnixEpoch,
  });
}

export function createApiKeyInTx(ctx: WriteCtx, args: CreateApiKeyArgs) {
  const ownerSubject = normalizeOwnerSubject(args.ownerSubject);
  const name = normalizeName(args.name);
  const scopesJson = normalizeScopesJson(args.scopesJson);
  const metadataJson = normalizeMetadataJson(args.metadataJson);
  const expiresAt = expiresAtFromSeconds(ctx, args.expiresInSeconds);
  const keyPrefix = normalizeKeyPrefix(args.keyPrefix);
  let activeKeys = 0;
  for (const row of ctx.db.apiKey.ownerSubject.filter(ownerSubject)) {
    if (row.status.tag === 'Active' && !isExpired(row, ctx.timestamp)) {
      activeKeys++;
      if (activeKeys >= MAX_ACTIVE_KEYS_PER_OWNER) {
        throwSenderError(errors.activeKeyLimitReached);
      }
    }
  }
  const { key, prefix } = generateRawKey(ctx, keyPrefix);
  const keyId = `ak_${ctx.newUuidV7().toString()}`;
  const row = ctx.db.apiKey.insert({
    keyId,
    prefix,
    hash: hashApiKey(key),
    ownerSubject,
    name,
    scopesJson,
    metadataJson,
    status: ApiKeyStatus.Active,
    createdAt: ctx.timestamp,
    createdAtOrder: -ctx.timestamp.microsSinceUnixEpoch,
    expiresAt,
    lastUsedAt: undefined,
    revokedAt: undefined,
  });
  recordUsage(ctx, {
    keyId: row.keyId,
    prefix: row.prefix,
    ownerSubject: row.ownerSubject,
    action: 'create',
    allowed: true,
    reason: 'created',
  });
  return toCreateResult(row, key);
}

function denied(
  ctx: WriteCtx,
  args: {
    keyId?: string | undefined;
    prefix?: string | undefined;
    ownerSubject?: string | undefined;
    action: string;
    reason: string;
    record?: boolean | undefined;
  }
): ApiKeyVerifyResult {
  if (args.record !== false) recordUsage(ctx, { ...args, allowed: false });
  return {
    allowed: false,
    reason: args.reason,
    keyId: undefined,
    prefix: args.prefix,
    ownerSubject: undefined,
    scopesJson: undefined,
    metadataJson: undefined,
  };
}

export function verifyApiKey(
  ctx: WriteCtx,
  args: VerifyApiKeyArgs
): ApiKeyVerifyResult {
  const requiredScope = normalizeRequiredScope(args.requiredScope);
  const action = normalizeAction(args.action, requiredScope);
  const key = args.key.trim();
  if (key.length === 0 || key.length > MAX_RAW_KEY_LENGTH) {
    return denied(ctx, { action, reason: 'invalid_key', record: false });
  }
  const prefix = extractLookupPrefix(key);
  if (prefix === undefined) {
    return denied(ctx, { action, reason: 'invalid_key', record: false });
  }
  const row = ctx.db.apiKey.prefix.find(prefix);
  if (!row) {
    return denied(ctx, {
      prefix,
      action,
      reason: 'unknown_key',
      record: false,
    });
  }
  if (!matchesApiKeyHash(key, row.hash)) {
    return denied(ctx, {
      keyId: row.keyId,
      prefix: row.prefix,
      ownerSubject: row.ownerSubject,
      action,
      reason: 'invalid_key',
    });
  }
  if (row.status.tag !== 'Active') {
    return denied(ctx, {
      keyId: row.keyId,
      prefix: row.prefix,
      ownerSubject: row.ownerSubject,
      action,
      reason: 'revoked',
    });
  }
  if (isExpired(row, ctx.timestamp)) {
    return denied(ctx, {
      keyId: row.keyId,
      prefix: row.prefix,
      ownerSubject: row.ownerSubject,
      action,
      reason: 'expired',
    });
  }
  if (!hasScope(row.scopesJson, requiredScope)) {
    return denied(ctx, {
      keyId: row.keyId,
      prefix: row.prefix,
      ownerSubject: row.ownerSubject,
      action,
      reason: 'scope_denied',
    });
  }
  ctx.db.apiKey.keyId.update({
    ...row,
    lastUsedAt: ctx.timestamp,
  });
  recordUsage(ctx, {
    keyId: row.keyId,
    prefix: row.prefix,
    ownerSubject: row.ownerSubject,
    action,
    allowed: true,
    reason: 'allowed',
  });
  return {
    allowed: true,
    reason: 'allowed',
    keyId: row.keyId,
    prefix: row.prefix,
    ownerSubject: row.ownerSubject,
    scopesJson: row.scopesJson,
    metadataJson: row.metadataJson,
  };
}

function canManageKey(ctx: WriteCtx, row: ApiKeyRow, subject: string) {
  return row.ownerSubject === subject || isAdmin(ctx);
}

export function revokeApiKeyInTx(
  ctx: WriteCtx,
  args: { keyId: string; ownerSubject?: string | undefined }
): void {
  const keyId = args.keyId.trim();
  if (!keyId) throwSenderError(errors.invalidKeyId);
  const row = ctx.db.apiKey.keyId.find(keyId);
  if (!row) throwSenderError(errors.notFound);
  const subject = normalizeOwnerSubject(
    args.ownerSubject ?? ctx.sender.toHexString()
  );
  if (!canManageKey(ctx, row, subject)) throwSenderError(errors.notAuthorized);
  if (row.status.tag === 'Revoked') return;
  ctx.db.apiKey.keyId.update({
    ...row,
    status: ApiKeyStatus.Revoked,
    revokedAt: ctx.timestamp,
  });
  recordUsage(ctx, {
    keyId: row.keyId,
    prefix: row.prefix,
    ownerSubject: row.ownerSubject,
    action: 'revoke',
    allowed: true,
    reason: 'revoked',
  });
}

export function rotateApiKeyInTx(
  ctx: WriteCtx,
  args: {
    keyId: string;
    ownerSubject?: string | undefined;
    expiresInSeconds?: number | undefined;
    keyPrefix?: string | undefined;
  }
) {
  const keyId = args.keyId.trim();
  if (!keyId) throwSenderError(errors.invalidKeyId);
  const row = ctx.db.apiKey.keyId.find(keyId);
  if (!row) throwSenderError(errors.notFound);
  const subject = normalizeOwnerSubject(
    args.ownerSubject ?? ctx.sender.toHexString()
  );
  if (!canManageKey(ctx, row, subject)) throwSenderError(errors.notAuthorized);
  // Only live keys rotate, so rotation never revives a key or exceeds the cap.
  if (row.status.tag !== 'Active') throwSenderError(errors.keyRevoked);
  if (isExpired(row, ctx.timestamp)) throwSenderError(errors.keyExpired);
  const keyPrefix = normalizeKeyPrefix(args.keyPrefix);
  const expiresAt =
    args.expiresInSeconds === undefined
      ? row.expiresAt
      : expiresAtFromSeconds(ctx, args.expiresInSeconds);
  const { key, prefix } = generateRawKey(ctx, keyPrefix);
  const updated = {
    ...row,
    prefix,
    hash: hashApiKey(key),
    expiresAt,
    lastUsedAt: undefined,
  };
  ctx.db.apiKey.keyId.update(updated);
  recordUsage(ctx, {
    keyId: row.keyId,
    prefix,
    ownerSubject: row.ownerSubject,
    action: 'rotate',
    allowed: true,
    reason: 'rotated',
  });
  return toCreateResult(updated, key);
}

export const createApiKey = spacetimedb.procedure(
  {
    name: t.string(),
    scopesJson: t.string(),
    metadataJson: t.option(t.string()),
    expiresInSeconds: t.option(t.u32()),
    keyPrefix: t.option(t.string()),
  },
  apiKeyCreateResult,
  (ctx, args) =>
    ctx.withTx(tx =>
      createApiKeyInTx(tx, {
        ownerSubject: ctx.sender.toHexString(),
        name: args.name,
        scopesJson: args.scopesJson,
        metadataJson: args.metadataJson,
        expiresInSeconds: args.expiresInSeconds,
        keyPrefix: args.keyPrefix,
      })
    )
);

export const createApiKeyForSubject = spacetimedb.procedure(
  {
    ownerSubject: t.string(),
    name: t.string(),
    scopesJson: t.string(),
    metadataJson: t.option(t.string()),
    expiresInSeconds: t.option(t.u32()),
    keyPrefix: t.option(t.string()),
  },
  apiKeyCreateResult,
  (ctx, args) =>
    ctx.withTx(tx => {
      requireAdmin(tx);
      return createApiKeyInTx(tx, args);
    })
);

export const rotateApiKey = spacetimedb.procedure(
  {
    keyId: t.string(),
    expiresInSeconds: t.option(t.u32()),
    keyPrefix: t.option(t.string()),
  },
  apiKeyCreateResult,
  (ctx, args) =>
    ctx.withTx(tx =>
      rotateApiKeyInTx(tx, {
        keyId: args.keyId,
        ownerSubject: ctx.sender.toHexString(),
        expiresInSeconds: args.expiresInSeconds,
        keyPrefix: args.keyPrefix,
      })
    )
);

export const revokeApiKey = spacetimedb.reducer(
  { keyId: t.string() },
  (ctx, args) => {
    revokeApiKeyInTx(ctx, {
      keyId: args.keyId,
      ownerSubject: ctx.sender.toHexString(),
    });
  }
);

export const revokeApiKeyForSubject = spacetimedb.reducer(
  { keyId: t.string(), ownerSubject: t.string() },
  (ctx, args) => {
    requireAdmin(ctx);
    revokeApiKeyInTx(ctx, args);
  }
);

export const setApiKeysConfig = spacetimedb.reducer(
  {
    /** Required on the first call. Generate it outside the module. */
    secret: t.option(t.string()),
    usageRetentionSeconds: t.option(t.u32()),
  },
  (ctx, args) => {
    requireAdmin(ctx);
    const existing = ctx.db.apiKeyConfig.singleton.find(true);
    const secret = args.secret ?? existing?.secret;
    if (secret === undefined) throwSenderError(errors.secretRequired);
    if (secret.length < MIN_SECRET_LENGTH) {
      throwSenderError(errors.invalidSecret);
    }
    const usageRetentionSeconds =
      args.usageRetentionSeconds ??
      existing?.usageRetentionSeconds ??
      DEFAULT_USAGE_RETENTION_SECONDS;
    if (
      usageRetentionSeconds < MIN_USAGE_RETENTION_SECONDS ||
      usageRetentionSeconds > MAX_EXPIRATION_SECONDS
    ) {
      throwSenderError(errors.invalidUsageRetention);
    }
    const row = {
      singleton: true,
      secret,
      counter: existing?.counter ?? 0n,
      usageRetentionSeconds,
      updatedAt: ctx.timestamp,
    };
    if (existing) ctx.db.apiKeyConfig.singleton.update(row);
    else ctx.db.apiKeyConfig.insert(row);
  }
);

export const addApiKeysAdmin = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, args) => {
    requireAdmin(ctx);
    if (ctx.db.apiKeyAdminIdentity.identity.find(args.identity) == null) {
      ctx.db.apiKeyAdminIdentity.insert({
        identity: args.identity,
        addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
      });
    }
  }
);

export const removeApiKeysAdmin = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, args) => {
    requireAdmin(ctx);
    const row = ctx.db.apiKeyAdminIdentity.identity.find(args.identity);
    if (!row) return;
    if (ctx.db.apiKeyAdminIdentity.count() <= 1n) {
      throwSenderError(errors.cannotRemoveLastAdmin);
    }
    ctx.db.apiKeyAdminIdentity.delete(row);
  }
);

/** Deletes a bounded batch of usage rows older than the retention window. */
export const apiKeysSweep = spacetimedb.reducer(
  { onSchedule: apiKeySweepTick },
  { arg: apiKeySweepTick.rowType },
  ctx => {
    const retention =
      ctx.db.apiKeyConfig.singleton.find(true)?.usageRetentionSeconds ??
      DEFAULT_USAGE_RETENTION_SECONDS;
    const cutoff = new Timestamp(
      ctx.timestamp.microsSinceUnixEpoch - BigInt(retention) * ONE_SECOND_MICROS
    );
    const expired = takeRows(
      ctx.db.apiKeyUsage.usedAt.filter(
        new Range(undefined, { tag: 'included', value: cutoff })
      ),
      USAGE_SWEEP_BATCH
    );
    for (const row of expired) ctx.db.apiKeyUsage.delete(row);
  }
);

export const myApiKeys = spacetimedb.view(
  { name: 'my_api_keys', public: true },
  t.array(apiKeySummary),
  ctx => {
    const subject = ctx.sender.toHexString();
    return takeRows(ctx.db.apiKey.ownerSubject.filter(subject), 500).map(
      toSummary
    );
  }
);

export const apiKeysAdmin = spacetimedb.view(
  { name: 'api_keys_admin', public: true },
  t.array(apiKeySummary),
  (ctx: ViewModuleCtx) => {
    if (!isAdmin(ctx)) return [];
    const rows = takeRows(
      ctx.db.apiKey.createdAtOrder.filter(new Range()),
      200
    );
    return rows.map(toSummary);
  }
);

export const apiKeyUsageAdmin = spacetimedb.view(
  { name: 'api_key_usage_admin', public: true },
  t.array(apiKeyUsageSummary),
  (ctx: ViewModuleCtx) => {
    if (!isAdmin(ctx)) return [];
    return takeRows(
      ctx.db.apiKeyUsage.usedAtOrder.filter(new Range()),
      100
    ).map(row => ({
      usageId: row.usageId,
      keyId: row.keyId,
      prefix: row.prefix,
      ownerSubject: row.ownerSubject,
      action: row.action,
      allowed: row.allowed,
      reason: row.reason,
      usedAt: row.usedAt,
    }));
  }
);
