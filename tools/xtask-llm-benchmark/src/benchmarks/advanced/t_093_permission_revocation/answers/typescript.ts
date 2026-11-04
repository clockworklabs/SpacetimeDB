import { schema, table, t, SenderError } from 'spacetimedb/server';

const privateDocument = table(
  { name: 'private_document' },
  {
    id: t.u64().primaryKey(),
    owner: t.identity(),
    title: t.string(),
    secretBody: t.string(),
  }
);
const readAccess = table(
  { name: 'read_access' },
  {
    reader: t.identity().primaryKey(),
    enabled: t.bool(),
  }
);
const spacetimedb = schema({ privateDocument, readAccess });
export default spacetimedb;

export const set_document = spacetimedb.reducer(
  { title: t.string(), secretBody: t.string() },
  (ctx, { title, secretBody }) => {
    const row = ctx.db.privateDocument.id.find(1n);
    if (row) {
      if (!row.owner.isEqual(ctx.sender)) throw new SenderError('owner only');
      ctx.db.privateDocument.id.update({ ...row, title, secretBody });
    } else
      ctx.db.privateDocument.insert({
        id: 1n,
        owner: ctx.sender,
        title,
        secretBody,
      });
  }
);

export const set_access = spacetimedb.reducer(
  { reader: t.identity(), enabled: t.bool() },
  (ctx, { reader, enabled }) => {
    const row = ctx.db.privateDocument.id.find(1n);
    if (!row || !row.owner.isEqual(ctx.sender))
      throw new SenderError('owner only');
    const access = { reader, enabled };
    if (ctx.db.readAccess.reader.find(reader))
      ctx.db.readAccess.reader.update(access);
    else ctx.db.readAccess.insert(access);
  }
);

const SafeDocument = t.row('SafeDocument', { id: t.u64(), title: t.string() });
export const visible_document = spacetimedb.view(
  { name: 'visible_document', public: true },
  t.array(SafeDocument),
  ctx => {
    const row = ctx.db.privateDocument.id.find(1n);
    const allowed = ctx.db.readAccess.reader.find(ctx.sender)?.enabled ?? false;
    return row && (row.owner.isEqual(ctx.sender) || allowed)
      ? [{ id: row.id, title: row.title }]
      : [];
  }
);
