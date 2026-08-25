import { Timestamp } from 'spacetimedb';
import { Range, type Infer } from 'spacetimedb/server';
import {
  ApiKeyStatus,
  apiKey,
  apiKeyCreateResult,
  apiKeySummary,
  apiKeyUsageSummary,
  spacetimedb,
  t,
  SenderError,
  type ReducerModuleCtx,
  type ViewModuleCtx,
  type WriteCtx,
} from './schema';
import { isAdmin, requireAdmin } from './auth';
import { sha256 } from '@spacetimedb/crypto';
import {
  base64Url,
  extractLookupPrefix,
  hashApiKey,
  hashMatches,
  hasScope,
  LOOKUP_SECRET_CHARS,
} from '../key-utils';

const DEFAULT_KEY_PREFIX = 'stdb_live';
const MAX_NAME_LENGTH = 120;
const MAX_OWNER_SUBJECT_LENGTH = 256;
const MAX_SCOPE_LENGTH = 128;
const MAX_SCOPES = 128;
const MAX_METADATA_JSON_LENGTH = 8192;
const MAX_RAW_KEY_LENGTH = 128;
const MAX_ACTIVE_KEYS_PER_OWNER = 50;
const MAX_EXPIRATION_SECONDS = 60 * 60 * 24 * 365 * 10;
const MAX_USAGE_SWEEP_ROWS = 1000;
const ONE_SECOND_MICROS = 1_000_000n;

const textEncoder = new TextEncoder();

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

function uniqueSecret(ctx: WriteCtx): string {
  const material = [
    ctx.newUuidV7().toString(),
    ctx.newUuidV7().toString(),
    ctx.timestamp.microsSinceUnixEpoch.toString(),
  ].join(':');
  // The key is `${keyPrefix}_${secret}` and lookup splits on the last '_', so
  // the secret must not contain '_' or the prefix lookup lands mid-secret and
  // verification fails. base64url can emit '_', so fold it to '-'.
  return base64Url(sha256(textEncoder.encode(material))).replace(/_/g, '-');
}

function normalizeKeyPrefix(prefix: string | undefined): string {
  const value = (prefix ?? DEFAULT_KEY_PREFIX).trim();
  if (!/^[A-Za-z][A-Za-z0-9_]{1,31}$/.test(value)) {
    throwSenderError('api_keys.invalid_key_prefix');
  }
  return value;
}

function generateRawKey(
  ctx: WriteCtx,
  keyPrefix: string
): { key: string; prefix: string } {
  const secret = uniqueSecret(ctx);
  const key = `${keyPrefix}_${secret}`;
  const prefix = `${keyPrefix}_${secret.slice(0, LOOKUP_SECRET_CHARS)}`;
  return { key, prefix };
}

function normalizeName(name: string): string {
  const value = name.trim().replace(/\s+/g, ' ');
  if (value.length === 0 || value.length > MAX_NAME_LENGTH) {
    throwSenderError('api_keys.invalid_name');
  }
  return value;
}

function normalizeOwnerSubject(ownerSubject: string): string {
  const value = ownerSubject.trim();
  if (value.length === 0 || value.length > MAX_OWNER_SUBJECT_LENGTH) {
    throwSenderError('api_keys.invalid_owner_subject');
  }
  return value;
}

function normalizeAction(
  action: string | undefined,
  requiredScope: string | undefined
): string {
  const value = (action ?? requiredScope ?? 'verify').trim();
  if (value.length === 0 || value.length > MAX_SCOPE_LENGTH) {
    throwSenderError('api_keys.invalid_action');
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
    throwSenderError('api_keys.invalid_required_scope');
  }
  return value;
}

function normalizeScopesJson(scopesJson: string): string {
  let parsed: unknown;
  try {
    parsed = JSON.parse(scopesJson);
  } catch {
    throwSenderError('api_keys.invalid_scopes_json');
  }
  if (!Array.isArray(parsed)) throwSenderError('api_keys.invalid_scopes_json');
  if (parsed.length === 0 || parsed.length > MAX_SCOPES) {
    throwSenderError('api_keys.invalid_scopes_json');
  }
  const seen = new Set<string>();
  const scopes: string[] = [];
  for (const raw of parsed) {
    if (typeof raw !== 'string')
      throwSenderError('api_keys.invalid_scopes_json');
    const scope = raw.trim();
    if (scope.length === 0 || scope.length > MAX_SCOPE_LENGTH) {
      throwSenderError('api_keys.invalid_scopes_json');
    }
    if (!/^[A-Za-z0-9:_*.-]+$/.test(scope)) {
      throwSenderError('api_keys.invalid_scopes_json');
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
    throwSenderError('api_keys.invalid_metadata_json');
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    throwSenderError('api_keys.invalid_metadata_json');
  }
  if (parsed === null || Array.isArray(parsed) || typeof parsed !== 'object') {
    throwSenderError('api_keys.invalid_metadata_json');
  }
  return JSON.stringify(parsed);
}

function senderSubject(sender: unknown): string {
  if (
    sender &&
    typeof (sender as { toHexString?: unknown }).toHexString === 'function'
  ) {
    return (sender as { toHexString: () => string }).toHexString();
  }
  return String(sender);
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
    throwSenderError('api_keys.invalid_expires_in_seconds');
  }
  return new Timestamp(
    (ctx.timestamp.microsSinceUnixEpoch as bigint) +
      BigInt(expiresInSeconds) * ONE_SECOND_MICROS
  );
}

function isExpired(row: ApiKeyRow, now: Timestamp): boolean {
  return (
    row.expiresAt !== undefined &&
    (row.expiresAt.microsSinceUnixEpoch as bigint) <=
      (now.microsSinceUnixEpoch as bigint)
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

export function createApiKey(ctx: WriteCtx, args: CreateApiKeyArgs) {
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
        throwSenderError('api_keys.active_key_limit_reached');
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
  if (!hashMatches(key, row.hash)) {
    return denied(ctx, {
      prefix,
      action,
      reason: 'invalid_key',
      record: false,
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

function canManageKey(
  ctx: ReducerModuleCtx | WriteCtx,
  row: ApiKeyRow,
  subject: string
): boolean {
  return row.ownerSubject === subject || isAdmin(ctx, ctx.sender);
}

export function revokeApiKey(
  ctx: WriteCtx,
  args: { keyId: string; ownerSubject?: string | undefined }
): void {
  const keyId = args.keyId.trim();
  if (!keyId) throwSenderError('api_keys.invalid_key_id');
  const row = ctx.db.apiKey.keyId.find(keyId);
  if (!row) throwSenderError('api_keys.not_found');
  const subject = normalizeOwnerSubject(
    args.ownerSubject ?? senderSubject(ctx.sender)
  );
  if (!canManageKey(ctx, row, subject))
    throwSenderError('api_keys.not_authorized');
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

export function rotateApiKey(
  ctx: WriteCtx,
  args: {
    keyId: string;
    ownerSubject?: string | undefined;
    expiresInSeconds?: number | undefined;
    keyPrefix?: string | undefined;
  }
) {
  const keyId = args.keyId.trim();
  if (!keyId) throwSenderError('api_keys.invalid_key_id');
  const row = ctx.db.apiKey.keyId.find(keyId);
  if (!row) throwSenderError('api_keys.not_found');
  const subject = normalizeOwnerSubject(
    args.ownerSubject ?? senderSubject(ctx.sender)
  );
  if (!canManageKey(ctx, row, subject))
    throwSenderError('api_keys.not_authorized');
  const keyPrefix = normalizeKeyPrefix(args.keyPrefix);
  const { key, prefix } = generateRawKey(ctx, keyPrefix);
  const expiresAt =
    args.expiresInSeconds === undefined
      ? row.expiresAt
      : expiresAtFromSeconds(ctx, args.expiresInSeconds);
  const updated = {
    ...row,
    prefix,
    hash: hashApiKey(key),
    status: ApiKeyStatus.Active,
    expiresAt,
    lastUsedAt: undefined,
    revokedAt: undefined,
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

export const create_api_key = spacetimedb.procedure(
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
      createApiKey(tx, {
        ownerSubject: senderSubject(ctx.sender),
        name: args.name,
        scopesJson: args.scopesJson,
        metadataJson: args.metadataJson,
        expiresInSeconds: args.expiresInSeconds,
        keyPrefix: args.keyPrefix,
      })
    )
);

export const create_api_key_for_subject = spacetimedb.procedure(
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
      requireAdmin(tx, ctx.sender);
      return createApiKey(tx, args);
    })
);

export const rotate_api_key = spacetimedb.procedure(
  {
    keyId: t.string(),
    expiresInSeconds: t.option(t.u32()),
    keyPrefix: t.option(t.string()),
  },
  apiKeyCreateResult,
  (ctx, args) =>
    ctx.withTx(tx =>
      rotateApiKey(tx, {
        keyId: args.keyId,
        ownerSubject: senderSubject(ctx.sender),
        expiresInSeconds: args.expiresInSeconds,
        keyPrefix: args.keyPrefix,
      })
    )
);

export const revoke_api_key = spacetimedb.reducer(
  { keyId: t.string() },
  (ctx, args) => {
    revokeApiKey(ctx, {
      keyId: args.keyId,
      ownerSubject: senderSubject(ctx.sender),
    });
  }
);

export const revoke_api_key_for_subject = spacetimedb.reducer(
  { keyId: t.string(), ownerSubject: t.string() },
  (ctx, args) => {
    requireAdmin(ctx);
    revokeApiKey(ctx, args);
  }
);

export const add_admin_identity = spacetimedb.reducer(
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

export const remove_admin_identity = spacetimedb.reducer(
  { identity: t.identity() },
  (ctx, args) => {
    requireAdmin(ctx);
    const row = ctx.db.apiKeyAdminIdentity.identity.find(args.identity);
    if (!row) return;
    if (ctx.db.apiKeyAdminIdentity.count() <= 1n) {
      throwSenderError('api_keys.cannot_remove_last_admin');
    }
    ctx.db.apiKeyAdminIdentity.delete(row);
  }
);

export const sweep_api_key_usage = spacetimedb.reducer(
  { maxAgeSeconds: t.u32(), maxRows: t.u32() },
  (ctx, args) => {
    requireAdmin(ctx);
    if (
      args.maxAgeSeconds < 3600 ||
      args.maxAgeSeconds > MAX_EXPIRATION_SECONDS ||
      args.maxRows < 1 ||
      args.maxRows > MAX_USAGE_SWEEP_ROWS
    ) {
      throwSenderError('api_keys.invalid_sweep_args');
    }
    const cutoff = new Timestamp(
      (ctx.timestamp.microsSinceUnixEpoch as bigint) -
        BigInt(args.maxAgeSeconds) * ONE_SECOND_MICROS
    );
    let examined = 0;
    for (const row of ctx.db.apiKeyUsage.usedAt.filter(
      new Range(undefined, { tag: 'included', value: cutoff })
    )) {
      if (examined >= args.maxRows) break;
      examined++;
      ctx.db.apiKeyUsage.usageId.delete(row.usageId);
    }
  }
);

export const myApiKeys = spacetimedb.view(
  { name: 'my_api_keys', public: true },
  t.array(apiKeySummary),
  ctx => {
    const subject = senderSubject(ctx.sender);
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
