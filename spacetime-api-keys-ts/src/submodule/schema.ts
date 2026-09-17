import {
  SenderError,
  schema,
  table,
  t,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
  type ViewCtx,
} from 'spacetimedb/server';
import { installApiKeys } from './install';

export const apiKeyStatus = t.enum('ApiKeyStatus', ['Active', 'Revoked']);

export const ApiKeyStatus = {
  Active: { tag: 'Active' as const },
  Revoked: { tag: 'Revoked' as const },
};

export const apiKey = table(
  { name: 'api_key', public: false },
  {
    keyId: t.string().primaryKey(),
    prefix: t.string().unique(),
    hash: t.string(),
    ownerSubject: t.string().index(),
    name: t.string(),
    scopesJson: t.string(),
    metadataJson: t.option(t.string()),
    status: apiKeyStatus.index(),
    createdAt: t.timestamp().index(),
    createdAtOrder: t.i64().index(),
    expiresAt: t.option(t.timestamp()),
    lastUsedAt: t.option(t.timestamp()),
    revokedAt: t.option(t.timestamp()),
  }
);

export const apiKeyAdminIdentity = table(
  { name: 'api_key_admin_identity', public: false },
  {
    identity: t.identity().primaryKey(),
    addedAtMicros: t.i64(),
  }
);

export const apiKeyUsage = table(
  { name: 'api_key_usage', public: false },
  {
    usageId: t.u64().primaryKey().autoInc(),
    keyId: t.string().index(),
    prefix: t.string(),
    ownerSubject: t.string().index(),
    action: t.string().index(),
    allowed: t.bool().index(),
    reason: t.string().index(),
    usedAt: t.timestamp().index(),
    usedAtOrder: t.i64().index(),
  }
);

export const apiKeySummary = t.object('ApiKeySummary', {
  keyId: t.string(),
  prefix: t.string(),
  ownerSubject: t.string(),
  name: t.string(),
  scopesJson: t.string(),
  metadataJson: t.option(t.string()),
  status: apiKeyStatus,
  createdAt: t.timestamp(),
  expiresAt: t.option(t.timestamp()),
  lastUsedAt: t.option(t.timestamp()),
  revokedAt: t.option(t.timestamp()),
});

export const apiKeyCreateResult = t.object('ApiKeyCreateResult', {
  keyId: t.string(),
  key: t.string(),
  prefix: t.string(),
  ownerSubject: t.string(),
  name: t.string(),
  scopesJson: t.string(),
  metadataJson: t.option(t.string()),
  status: apiKeyStatus,
  createdAt: t.timestamp(),
  expiresAt: t.option(t.timestamp()),
});

export const apiKeyUsageSummary = t.object('ApiKeyUsageSummary', {
  usageId: t.u64(),
  keyId: t.string(),
  prefix: t.string(),
  ownerSubject: t.string(),
  action: t.string(),
  allowed: t.bool(),
  reason: t.string(),
  usedAt: t.timestamp(),
});

export const apiKeyVerifyResult = t.object('ApiKeyVerifyResult', {
  allowed: t.bool(),
  reason: t.string(),
  keyId: t.option(t.string()),
  prefix: t.option(t.string()),
  ownerSubject: t.option(t.string()),
  scopesJson: t.option(t.string()),
  metadataJson: t.option(t.string()),
});

export const spacetimedb = schema({
  apiKey,
  apiKeyAdminIdentity,
  apiKeyUsage,
});

export const init = spacetimedb.init(ctx => {
  installApiKeys(ctx);
});

export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
export type ReducerModuleCtx = ReducerCtx<Schema>;
export type ProcedureModuleCtx = ProcedureCtx<Schema>;
export type TransactionModuleCtx = TransactionCtx<Schema>;
export type ViewModuleCtx = ViewCtx<Schema>;
export type WriteCtx = ReducerModuleCtx | TransactionModuleCtx;

export { SenderError, t };
