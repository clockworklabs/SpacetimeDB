import { schema, table, t, SenderError } from 'spacetimedb/server';
import { Timestamp } from 'spacetimedb';
const searchLog = table(
  {
    name: 'search_log',
    public: true,
    indexes: [
      {
        accessor: 'byRequest',
        algorithm: 'btree',
        columns: ['owner', 'requestId'],
      },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    owner: t.identity(),
    requestId: t.string(),
    term: t.string(),
  }
);
const searchBucket = table(
  { name: 'search_bucket' },
  {
    owner: t.identity().primaryKey(),
    tokens: t.u32(),
    refilledAt: t.timestamp(),
  }
);
const spacetimedb = schema({ searchLog, searchBucket });
export default spacetimedb;
export const log_search = spacetimedb.reducer(
  { requestId: t.string(), term: t.string() },
  (ctx, { requestId, term }) => {
    if (!term) throw new SenderError('empty term');
    const previous = [
      ...ctx.db.searchLog.byRequest.filter([ctx.sender, requestId]),
    ][0];
    if (previous) {
      if (previous.term !== term) throw new SenderError('request conflict');
      return;
    }
    const old = ctx.db.searchBucket.owner.find(ctx.sender);
    const bucket = old
      ? { ...old }
      : { owner: ctx.sender, tokens: 3, refilledAt: ctx.timestamp };
    const intervals =
      (ctx.timestamp.microsSinceUnixEpoch -
        bucket.refilledAt.microsSinceUnixEpoch) /
      5_000_000n;
    if (intervals > 0n) {
      bucket.tokens = Math.min(3, bucket.tokens + Number(intervals));
      bucket.refilledAt = new Timestamp(
        bucket.refilledAt.microsSinceUnixEpoch + intervals * 5_000_000n
      );
    }
    if (!bucket.tokens) throw new SenderError('rate limited');
    bucket.tokens--;
    if (old) ctx.db.searchBucket.owner.update(bucket);
    else ctx.db.searchBucket.insert(bucket);
    ctx.db.searchLog.insert({ id: 0n, owner: ctx.sender, requestId, term });
  }
);
