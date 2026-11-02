import { schema, table, t, SenderError } from 'spacetimedb/server';

const auditEntry = table(
  { name: 'audit_entry', public: true },
  {
    id: t.u64().primaryKey(),
    tenant: t.string(),
    occurredAt: t.u64(),
    visible: t.bool(),
  }
);
const pageEntry = table(
  { name: 'page_entry', public: true },
  {
    position: t.u64().primaryKey(),
    entryId: t.u64(),
    occurredAt: t.u64(),
  }
);
const spacetimedb = schema({ auditEntry, pageEntry });
export default spacetimedb;

export const add_entry = spacetimedb.reducer(
  { id: t.u64(), tenant: t.string(), occurredAt: t.u64(), visible: t.bool() },
  (ctx, { id, tenant, occurredAt, visible }) => {
    ctx.db.auditEntry.insert({ id, tenant, occurredAt, visible });
  }
);

export const read_page = spacetimedb.reducer(
  { tenant: t.string(), afterTime: t.u64(), afterId: t.u64(), limit: t.u64() },
  (ctx, { tenant, afterTime, afterId, limit }) => {
    if (limit === 0n || limit > 10n) throw new SenderError('invalid limit');
    const rows = [...ctx.db.auditEntry.iter()].filter(
      r =>
        r.tenant === tenant &&
        r.visible &&
        (r.occurredAt > afterTime ||
          (r.occurredAt === afterTime && r.id > afterId))
    );
    rows.sort((a, b) =>
      a.occurredAt < b.occurredAt
        ? -1
        : a.occurredAt > b.occurredAt
          ? 1
          : a.id < b.id
            ? -1
            : a.id > b.id
              ? 1
              : 0
    );
    for (const old of ctx.db.pageEntry.iter())
      ctx.db.pageEntry.position.delete(old.position);
    rows.slice(0, Number(limit)).forEach((r, i) =>
      ctx.db.pageEntry.insert({
        position: BigInt(i),
        entryId: r.id,
        occurredAt: r.occurredAt,
      })
    );
  }
);
