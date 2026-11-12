import { schema, table, t, SenderError } from 'spacetimedb/server';
const privateNote = table(
  { name: 'private_note' },
  {
    id: t.u64().primaryKey(),
    owner: t.identity().index('btree'),
    tag: t.string(),
    text: t.string(),
  }
);
const spacetimedb = schema({ privateNote });
export default spacetimedb;
export const put_note = spacetimedb.reducer(
  { id: t.u64(), tag: t.string(), text: t.string() },
  (ctx, { id, tag, text }) => {
    const old = ctx.db.privateNote.id.find(id);
    if (old && !old.owner.equals(ctx.sender))
      throw new SenderError('owner only');
    const row = { id, owner: ctx.sender, tag, text };
    if (old) ctx.db.privateNote.id.update(row);
    else ctx.db.privateNote.insert(row);
  }
);
export const remove_note = spacetimedb.reducer(
  { id: t.u64() },
  (ctx, { id }) => {
    const old = ctx.db.privateNote.id.find(id);
    if (old && !old.owner.equals(ctx.sender))
      throw new SenderError('owner only');
    if (old) ctx.db.privateNote.id.delete(id);
  }
);
const NoteResult = t.row('NoteResult', { id: t.u64(), text: t.string() });
export const notesForTag = spacetimedb.view(
  { name: 'notes_for_tag', public: true },
  { tag: t.string() },
  t.array(NoteResult),
  (ctx, { tag }) =>
    [...ctx.db.privateNote.owner.filter(ctx.sender)]
      .filter(row => row.tag === tag)
      .map(({ id, text }) => ({ id, text }))
);
