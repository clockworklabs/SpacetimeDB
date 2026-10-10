import { schema, table, t, SenderError } from 'spacetimedb/server';

const legacyItem = table(
  { name: 'legacy_item', public: true },
  {
    id: t.u64().primaryKey(),
    value: t.string(),
    revision: t.u64(),
    deleted: t.bool(),
  }
);
const backfillSnapshot = table(
  { name: 'backfill_snapshot', public: true },
  {
    id: t.u64().primaryKey(),
    value: t.string(),
    revision: t.u64(),
    deleted: t.bool(),
  }
);
const itemV2 = table(
  { name: 'item_v2', public: true },
  {
    id: t.u64().primaryKey(),
    value: t.string(),
    revision: t.u64(),
  }
);
const spacetimedb = schema({ legacyItem, backfillSnapshot, itemV2 });
export default spacetimedb;

export const write_item = spacetimedb.reducer(
  { id: t.u64(), value: t.string() },
  (ctx, { id, value }) => {
    const revision = (ctx.db.legacyItem.id.find(id)?.revision ?? 0n) + 1n;
    const row = { id, value, revision, deleted: false };
    if (ctx.db.legacyItem.id.find(id)) ctx.db.legacyItem.id.update(row);
    else ctx.db.legacyItem.insert(row);
    const next = { id, value, revision };
    if (ctx.db.itemV2.id.find(id)) ctx.db.itemV2.id.update(next);
    else ctx.db.itemV2.insert(next);
  }
);

export const seed_legacy = spacetimedb.reducer(
  { id: t.u64(), value: t.string() },
  (ctx, { id, value }) => {
    ctx.db.legacyItem.insert({ id, value, revision: 1n, deleted: false });
  }
);

export const delete_item = spacetimedb.reducer(
  { id: t.u64() },
  (ctx, { id }) => {
    const row = ctx.db.legacyItem.id.find(id);
    if (row && !row.deleted) {
      ctx.db.legacyItem.id.update({
        ...row,
        revision: row.revision + 1n,
        deleted: true,
      });
      ctx.db.itemV2.id.delete(id);
    }
  }
);

export const capture_batch = spacetimedb.reducer(
  { afterId: t.u64(), limit: t.u64() },
  (ctx, { afterId, limit }) => {
    if (limit === 0n || limit > 10n) throw new SenderError('invalid limit');
    const rows = [...ctx.db.legacyItem.iter()]
      .filter(r => r.id > afterId)
      .sort((a, b) => (a.id < b.id ? -1 : 1))
      .slice(0, Number(limit));
    for (const row of ctx.db.backfillSnapshot.iter())
      ctx.db.backfillSnapshot.id.delete(row.id);
    for (const row of rows) ctx.db.backfillSnapshot.insert(row);
  }
);

export const apply_batch = spacetimedb.reducer({}, (ctx, {}) => {
  for (const snapshot of ctx.db.backfillSnapshot.iter()) {
    const r = ctx.db.legacyItem.id.find(snapshot.id);
    if (!r) throw new SenderError('missing source');
    if (r.deleted) ctx.db.itemV2.id.delete(r.id);
    else {
      const next = { id: r.id, value: r.value, revision: r.revision };
      if (ctx.db.itemV2.id.find(r.id)) ctx.db.itemV2.id.update(next);
      else ctx.db.itemV2.insert(next);
    }
  }
});
