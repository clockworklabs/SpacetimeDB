import { schema, table, t } from 'spacetimedb/server';
const SourceViewRow = t.row('SourceViewRow', { id: t.u64().primaryKey(), value: t.string(), visible: t.bool().index('btree') });
const sourceRow = table({ name: 'source_row', public: true }, SourceViewRow);
const spacetimedb = schema({ sourceRow }); export default spacetimedb;
export const source_view = spacetimedb.anonymousView({ name: 'source_view', public: true }, t.array(SourceViewRow), ctx => Array.from(ctx.db.sourceRow.visible.filter(true)));
