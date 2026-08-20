import { schema, table, t } from 'spacetimedb/server';

const member = table({ name: 'member', public: true }, { id: t.u64().primaryKey(), name: t.string() });
const eligibility = table({ name: 'eligibility', public: true }, { id: t.u64().primaryKey(), memberId: t.u64().index('btree') });
const spacetimedb = schema({ member, eligibility });
export default spacetimedb;
export const seed = spacetimedb.reducer(ctx => {
  ctx.db.member.insert({ id: 1n, name: 'Ada' }); ctx.db.member.insert({ id: 2n, name: 'Grace' });
  ctx.db.eligibility.insert({ id: 10n, memberId: 1n });
});
export const eligible_member = spacetimedb.view(
  { name: 'eligible_member', public: true }, t.array(member.rowType),
  ctx => ctx.from.eligibility.rightSemijoin(ctx.from.member, (eligibility, member) => eligibility.memberId.eq(member.id))
);
