import { schema, table, t } from 'spacetimedb/server';
const ticket = table({ name: 'ticket', public: true }, { id: t.u64().primaryKey(), status: t.string().index('btree'), title: t.string() });
const spacetimedb = schema({ ticket }); export default spacetimedb;
export const seed = spacetimedb.reducer(ctx => { ctx.db.ticket.insert({ id: 1n, status: 'open', title: 'A' }); ctx.db.ticket.insert({ id: 2n, status: 'closed', title: 'B' }); });
export const open_ticket = spacetimedb.view({ name: 'open_ticket', public: true }, t.array(ticket.rowType), ctx => ctx.from.ticket.where(ticket => ticket.status.eq('open')));
