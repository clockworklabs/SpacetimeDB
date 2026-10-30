import {
  schema,
  t,
  type InferSchema,
  type TransactionCtx,
  type ProcedureCtx,
} from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import * as s3 from '@spacetimedb/s3/submodule';
import {
  client,
  s3Tick,
  uploadReservation,
  downloadGrant,
} from '@spacetimedb/s3';

const db = schema(
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
      TEST_ENDPOINT: t.option(t.string()),
      TEST_FAST: t.option(t.string()),
    },
  }
);
export default db;
type Tx = TransactionCtx<InferSchema<typeof db>>;
type Context = ProcedureCtx<InferSchema<typeof db>>;
function files(env: Context['env']) {
  return client({
    submodule: (tx: Tx) => tx.as.s3,
    bucket: env.S3_BUCKET,
    region: env.S3_REGION,
    prefix: env.S3_PREFIX,
    accountId: env.S3_ACCOUNT_ID,
    ...(env.TEST_FAST === 'true'
      ? {
          maxFileBytes: 128,
          maxBytesPerOwner: 128,
          maxFilesPerOwner: 4,
          uploadSeconds: 10,
          recoverySeconds: 30,
          receiptSeconds: 5,
          sweepSeconds: 1,
          httpTimeoutSeconds: 1,
        }
      : {}),
  });
}
function credentials(ctx: Context) {
  return {
    accessKeyId: ctx.env.AWS_ACCESS_KEY_ID,
    secretAccessKey: ctx.env.AWS_SECRET_ACCESS_KEY,
    sessionToken: ctx.env.AWS_SESSION_TOKEN,
    expiresAt: ctx.env.AWS_SESSION_EXPIRES_AT
      ? Number(ctx.env.AWS_SESSION_EXPIRES_AT)
      : undefined,
  };
}
function procedure(ctx: Context) {
  const endpoint = ctx.env.TEST_ENDPOINT;
  if (!endpoint) return ctx;
  // Only the local fixture redirects requests; the package accepts AWS HTTPS targets.
  if (
    !/^http:\/\/127\.0\.0\.1:\d+$/.test(endpoint) ||
    ctx.env.AWS_ACCESS_KEY_ID !== 'AKIAIOSFODNN7EXAMPLE'
  )
    throw new Error('Invalid fault-test environment');
  const origin = `https://${ctx.env.S3_BUCKET}.s3.${ctx.env.S3_REGION}.amazonaws.com`;
  const http: Context['http'] = {
    fetch(url, options) {
      const target = String(url);
      if (!target.startsWith(`${origin}/`))
        throw new Error('Unexpected S3 target');
      return ctx.http.fetch(endpoint + target.slice(origin.length), options);
    },
  };
  return { http, withTx: <T>(body: (tx: Tx) => T) => ctx.withTx(body) };
}
export const init = db.init(tx => {
  files(tx.env).initialize(tx);
  tx.db.s3Tick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(
      tx.env.TEST_FAST === 'true' ? 500_000n : 5_000_000n
    ),
  });
});
export const maintain = db.procedure(
  { onSchedule: s3Tick },
  { tick: s3Tick.rowType },
  t.unit(),
  ctx => {
    files(ctx.env).reconcile(procedure(ctx), credentials(ctx));
    return {};
  }
);
export const beginUpload = db.procedure(
  {
    requestId: t.string(),
    filename: t.string(),
    contentType: t.string(),
    size: t.u32(),
    checksum: t.string(),
  },
  uploadReservation,
  (ctx, args) =>
    files(ctx.env).beginUpload(procedure(ctx), credentials(ctx), {
      ...args,
      owner: ctx.sender.toHexString(),
      requestKey: args.requestId,
    })
);
export const completeUpload = db.procedure(
  { id: t.string() },
  t.unit(),
  (ctx, { id }) => {
    files(ctx.env).completeUpload(procedure(ctx), credentials(ctx), {
      owner: ctx.sender.toHexString(),
      id,
    });
    return {};
  }
);
export const getDownloadUrl = db.procedure(
  { id: t.string() },
  downloadGrant,
  (ctx, { id }) =>
    files(ctx.env).getDownloadUrl(procedure(ctx), credentials(ctx), {
      owner: ctx.sender.toHexString(),
      id,
    })
);
export const deleteFile = db.reducer({ id: t.string() }, (tx, { id }) =>
  files(tx.env).deleteFile(tx, { owner: tx.sender.toHexString(), id })
);
export const myFiles = db.view(
  { name: 'my_files', public: true },
  t.array(
    t.object('FileMetadata', {
      id: t.string(),
      filename: t.string(),
      contentType: t.string(),
      size: t.u32(),
      state: t.string(),
    })
  ),
  ctx =>
    (['Pending', 'Copying', 'Ready', 'Deleting'] as const)
      .flatMap(tag => {
        // Keep the view bounded by active reservations, excluding retained receipts.
        return [
          ...ctx.db.s3.file.byOwnerState.filter([
            ctx.sender.toHexString(),
            { tag },
          ]),
        ];
      })
      .map(({ id, filename, contentType, size, state }) => ({
        id: String(id),
        filename,
        contentType,
        size,
        state: state.tag.toLowerCase(),
      }))
);
