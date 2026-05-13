import { schema, table, t } from 'spacetimedb/server';

const document = table({
  name: 'document',
  public: true,
}, {
  id: t.u64().primaryKey().autoInc(),
  owner: t.identity().index('btree'),
  title: t.string(),
});

const document_share = table({
  name: 'document_share',
  public: true,
}, {
  document_id: t.u64().index('btree'),
  shared_with: t.identity().index('btree'),
});

const spacetimedb = schema({ document, document_share });
export default spacetimedb;

export const create_document = spacetimedb.reducer(
  { title: t.string() },
  (ctx, { title }) => {
    ctx.db.document.insert({ id: 0n, owner: ctx.sender, title });
  }
);

export const share_document = spacetimedb.reducer(
  { document_id: t.u64(), target: t.identity() },
  (ctx, { document_id, target }) => {
    const doc = ctx.db.document.id.find(document_id);
    if (!doc) throw new Error('not found');
    if (!doc.owner.equals(ctx.sender)) throw new Error('not owner');
    ctx.db.document_share.insert({ document_id, shared_with: target });
  }
);

export const edit_document = spacetimedb.reducer(
  { document_id: t.u64(), new_title: t.string() },
  (ctx, { document_id, new_title }) => {
    const doc = ctx.db.document.id.find(document_id);
    if (!doc) throw new Error('not found');
    const isOwner = doc.owner.equals(ctx.sender);
    const isShared = [...ctx.db.document_share.document_id.filter(document_id)]
      .some(s => s.shared_with.equals(ctx.sender));
    if (!isOwner && !isShared) throw new Error('unauthorized');
    ctx.db.document.id.update({ ...doc, title: new_title });
  }
);
