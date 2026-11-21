import {
  schema,
  table,
  t,
  type InferSchema,
  type ProcedureCtx,
  type ReducerCtx,
  type TransactionCtx,
  type ViewCtx,
  type SumBuilder,
  type SumColumnBuilder,
} from 'spacetimedb/server';

type ApiKeyStatusVariants = Record<
  'Active' | 'Revoked',
  ReturnType<typeof t.unit>
>;
type ApiKeyStatusBuilder = SumBuilder<ApiKeyStatusVariants>;
export const apiKeyStatus: ApiKeyStatusBuilder = t.enum('ApiKeyStatus', {
  Active: t.unit(),
  Revoked: t.unit(),
});
const apiKeyStatusIndexed: SumColumnBuilder<
  ApiKeyStatusVariants,
  { indexType: 'btree' }
> = apiKeyStatus.index();

export const ApiKeyStatus = {
  Active: { tag: 'Active' as const },
  Revoked: { tag: 'Revoked' as const },
};

const apiKeyOptions = { name: 'api_key', public: false };
const apiKeyColumns = {
  keyId: t.string().primaryKey(),
  prefix: t.string().unique(),
  hash: t.string(),
  ownerSubject: t.string().index(),
  name: t.string(),
  scopesJson: t.string(),
  metadataJson: t.option(t.string()),
  status: apiKeyStatusIndexed,
  createdAt: t.timestamp().index(),
  createdAtOrder: t.i64().index(),
  expiresAt: t.option(t.timestamp()),
  lastUsedAt: t.option(t.timestamp()),
  revokedAt: t.option(t.timestamp()),
};
export const apiKey: ReturnType<
  typeof table<typeof apiKeyColumns, typeof apiKeyOptions>
> = table(apiKeyOptions, apiKeyColumns);

const apiKeyAdminIdentityOptions = {
  name: 'api_key_admin_identity',
  public: false,
};
const apiKeyAdminIdentityColumns = {
  identity: t.identity().primaryKey(),
  addedAt: t.timestamp(),
};
export const apiKeyAdminIdentity: ReturnType<
  typeof table<
    typeof apiKeyAdminIdentityColumns,
    typeof apiKeyAdminIdentityOptions
  >
> = table(apiKeyAdminIdentityOptions, apiKeyAdminIdentityColumns);

// Created by setApiKeysConfig. `counter` advances with every derived secret.
const apiKeyConfigOptions = { name: 'api_key_config', public: false };
const apiKeyConfigColumns = {
  singleton: t.bool().primaryKey(),
  secret: t.string(),
  counter: t.u64(),
  usageRetentionSeconds: t.u32(),
  updatedAt: t.timestamp(),
};
export const apiKeyConfig: ReturnType<
  typeof table<typeof apiKeyConfigColumns, typeof apiKeyConfigOptions>
> = table(apiKeyConfigOptions, apiKeyConfigColumns);

const apiKeySweepTickOptions = { name: 'api_key_sweep_tick' };
const apiKeySweepTickColumns = {
  scheduledId: t.u64().primaryKey().autoInc(),
  scheduledAt: t.scheduleAt(),
};
export const apiKeySweepTick: ReturnType<
  typeof table<typeof apiKeySweepTickColumns, typeof apiKeySweepTickOptions>
> = table(apiKeySweepTickOptions, apiKeySweepTickColumns);

const apiKeyUsageOptions = { name: 'api_key_usage', public: false };
const apiKeyUsageColumns = {
  usageId: t.u64().primaryKey().autoInc(),
  keyId: t.string().index(),
  prefix: t.string(),
  ownerSubject: t.string().index(),
  action: t.string().index(),
  allowed: t.bool().index(),
  reason: t.string().index(),
  usedAt: t.timestamp().index(),
  usedAtOrder: t.i64().index(),
};
export const apiKeyUsage: ReturnType<
  typeof table<typeof apiKeyUsageColumns, typeof apiKeyUsageOptions>
> = table(apiKeyUsageOptions, apiKeyUsageColumns);

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

const tables: {
  apiKey: typeof apiKey;
  apiKeyAdminIdentity: typeof apiKeyAdminIdentity;
  apiKeyConfig: typeof apiKeyConfig;
  apiKeySweepTick: typeof apiKeySweepTick;
  apiKeyUsage: typeof apiKeyUsage;
} = {
  apiKey,
  apiKeyAdminIdentity,
  apiKeyConfig,
  apiKeySweepTick,
  apiKeyUsage,
};
export const spacetimedb: ReturnType<typeof schema<typeof tables>> =
  schema(tables);

export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
export type ReducerModuleCtx = ReducerCtx<Schema>;
export type ProcedureModuleCtx = ProcedureCtx<Schema>;
export type TransactionModuleCtx = TransactionCtx<Schema>;
export type ViewModuleCtx = ViewCtx<Schema>;
export type WriteCtx = ReducerModuleCtx | TransactionModuleCtx;

export { t };
