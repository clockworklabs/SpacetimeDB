import { schema, table, t } from 'spacetimedb/server';
const content = table({ name: 'content', public: true }, { id: t.u64().primaryKey(), category: t.string().index('btree'), active: t.bool(), score: t.i32() });
const spacetimedb = schema({ content }); export default spacetimedb;
export const seed = spacetimedb.reducer(ctx => {
  ctx.db.content.insert({ id: 1n, category: 'news', active: true, score: 20 });
  ctx.db.content.insert({ id: 2n, category: 'news', active: false, score: 20 });
  ctx.db.content.insert({ id: 3n, category: 'news', active: true, score: 5 });
  ctx.db.content.insert({ id: 4n, category: 'sports', active: true, score: 20 });
});
export const featured_content = spacetimedb.anonymousView(
  { name: 'featured_content', public: true }, t.array(content.rowType),
  ctx => Array.from(ctx.db.content.category.filter('news')).filter(row => row.active && row.score >= 10)
);
