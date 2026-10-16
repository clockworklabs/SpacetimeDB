import {
  schema,
  t,
  type InferSchema,
  type ProcedureCtx,
  type TransactionCtx,
} from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import * as s3 from '@spacetimedb/s3/submodule';
import {
  client,
  s3Tick,
  uploadReservation,
  downloadGrant,
} from '@spacetimedb/s3';

const spacetimedb = schema(
  { s3, s3Tick },
  {
    env: {
      S3_BUCKET: t.string(),
      S3_REGION: t.string(),
      S3_PREFIX: t.string(),
      S3_ACCOUNT_ID: t.string(),
      AWS_ACCESS_KEY_ID: t.string(),
      AWS_SECRET_ACCESS_KEY: t.string(),
      AWS_SESSION_TOKEN: t.option(t.string()),
      AWS_SESSION_EXPIRES_AT: t.option(t.string()),
    },
  }
);
export default spacetimedb;

type Tx = TransactionCtx<InferSchema<typeof spacetimedb>>;
type Ctx = ProcedureCtx<InferSchema<typeof spacetimedb>>;

function files(env: Ctx['env']) {
  return client({
    submodule: (tx: Tx) => tx.as.s3,
    bucket: env.S3_BUCKET,
    region: env.S3_REGION,
    prefix: env.S3_PREFIX,
    accountId: env.S3_ACCOUNT_ID,
  });
}

function credentials(env: Ctx['env']) {
  return {
    accessKeyId: env.AWS_ACCESS_KEY_ID,
    secretAccessKey: env.AWS_SECRET_ACCESS_KEY,
    sessionToken: env.AWS_SESSION_TOKEN,
    expiresAt: env.AWS_SESSION_EXPIRES_AT
      ? Number(env.AWS_SESSION_EXPIRES_AT)
      : undefined,
  };
}

// Each signed-in identity owns its own files.
const owner = (ctx: { sender: { toHexString(): string } }) =>
  ctx.sender.toHexString();

export const init = spacetimedb.init(tx => {
  files(tx.env).initialize(tx);
  tx.db.s3Tick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(5_000_000n),
  });
});

export const maintain = spacetimedb.procedure(
  { onSchedule: s3Tick },
  { tick: s3Tick.rowType },
  t.unit(),
  ctx => {
    files(ctx.env).reconcile(ctx, credentials(ctx.env));
    return {};
  }
);

export const beginUpload = spacetimedb.procedure(
  {
    requestId: t.string(),
    filename: t.string(),
    contentType: t.string(),
    size: t.u32(),
    checksum: t.string(),
  },
  uploadReservation,
  (ctx, { requestId, ...file }) =>
    files(ctx.env).beginUpload(ctx, credentials(ctx.env), {
      ...file,
      owner: owner(ctx),
      requestKey: requestId,
    })
);

export const completeUpload = spacetimedb.procedure(
  { id: t.string() },
  t.unit(),
  (ctx, { id }) => {
    files(ctx.env).completeUpload(ctx, credentials(ctx.env), {
      owner: owner(ctx),
      id,
    });
    return {};
  }
);

export const getDownloadUrl = spacetimedb.procedure(
  { id: t.string() },
  downloadGrant,
  (ctx, { id }) =>
    files(ctx.env).getDownloadUrl(ctx, credentials(ctx.env), {
      owner: owner(ctx),
      id,
    })
);

export const deleteFile = spacetimedb.reducer(
  { id: t.string() },
  (tx, { id }) => files(tx.env).deleteFile(tx, { owner: owner(tx), id })
);

export const myFiles = spacetimedb.view(
  { name: 'my_files', public: true },
  t.array(
    t.object('MyFile', {
      id: t.string(),
      filename: t.string(),
      contentType: t.string(),
      size: t.u32(),
      state: t.string(),
      createdAt: t.timestamp(),
    })
  ),
  ctx =>
    (['Pending', 'Copying', 'Ready', 'Deleting'] as const)
      .flatMap(tag => [
        ...ctx.db.s3.file.byOwnerState.filter([owner(ctx), { tag }]),
      ])
      .map(({ id, filename, contentType, size, state, createdAt }) => ({
        id: String(id),
        filename,
        contentType,
        size,
        state: state.tag.toLowerCase(),
        createdAt,
      }))
);
