import { schema, table, t } from 'spacetimedb/server';

const secretNote = table({ name: 'secret_note' }, {
  id: t.u64().primaryKey(),
  owner: t.identity().index('btree'),
  title: t.string(),
  secretBody: t.string(),
});
const SafeNote = t.row('SafeNoteRow', { id: t.u64(), title: t.string() });
const spacetimedb = schema({ secretNote });
export default spacetimedb;

export const seed_private_note = spacetimedb.reducer(ctx => {
  ctx.db.secretNote.insert({ id: 1n, owner: ctx.sender, title: 'Visible title', secretBody: 'never expose this' });
});

export const my_safe_note = spacetimedb.view(
  { name: 'my_safe_note', public: true }, t.array(SafeNote),
  ctx => Array.from(ctx.db.secretNote.owner.filter(ctx.sender)).map(note => ({ id: note.id, title: note.title }))
);
