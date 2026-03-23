import { schema, table, t } from 'spacetimedb/server';

const author = table({
  name: 'author',
}, {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
});

const post = table({
  name: 'post',
}, {
  id: t.u64().primaryKey().autoInc(),
  authorId: t.u64().index('btree'),
  title: t.string(),
});

const spacetimedb = schema({ author, post });
export default spacetimedb;

export const delete_author = spacetimedb.reducer(
  { authorId: t.u64() },
  (ctx, { authorId }) => {
    // Delete all posts by this author
    for (const p of ctx.db.post.authorId.filter(authorId)) {
      ctx.db.post.id.delete(p.id);
    }
    // Delete the author
    ctx.db.author.id.delete(authorId);
  }
);
