import { schema, table, t } from 'spacetimedb/server';

const cacheEntry = table(
  { name: 'cache_entry', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    product: t.string(),
    language: t.string(),
    value: t.string(),
    expiresAt: t.i64(),
  }
);
const spacetimedb = schema({ cacheEntry });
export default spacetimedb;

export const fetch_cached = spacetimedb.procedure(
  {
    product: t.string(),
    language: t.string(),
    ttlMs: t.u64(),
    url: t.string(),
  },
  t.string(),
  (ctx, { product, language, ttlMs, url }) => {
    if (!product || !language || ttlMs > 60000n) return 'invalid input';
    const now = ctx.timestamp.microsSinceUnixEpoch;
    if (ttlMs > 0n) {
      const cached = ctx.withTx(tx =>
        [...tx.db.cacheEntry.iter()].find(
          r =>
            r.product === product &&
            r.language === language &&
            r.expiresAt > now
        )
      );
      if (cached) return cached.value;
    }
    let value: string;
    try {
      const response = ctx.http.fetch(url);
      if (response.status !== 200) return 'upstream error';
      value = response.text();
    } catch {
      return 'upstream error';
    }
    if (ttlMs > 0n)
      ctx.withTx(tx => {
        const old = [...tx.db.cacheEntry.iter()].find(
          r => r.product === product && r.language === language
        );
        const next = {
          id: old?.id ?? 0n,
          product,
          language,
          value,
          expiresAt: tx.timestamp.microsSinceUnixEpoch + ttlMs * 1000n,
        };
        if (old) tx.db.cacheEntry.id.update(next);
        else tx.db.cacheEntry.insert(next);
      });
    return value;
  }
);
