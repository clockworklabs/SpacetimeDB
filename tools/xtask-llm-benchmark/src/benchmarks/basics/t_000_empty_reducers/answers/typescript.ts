import { schema, table, t } from 'spacetimedb/server';

const emptyTable = table({ name: 'empty_table' }, { id: t.i32().primaryKey() });

const spacetimedb = schema({ emptyTable });
export default spacetimedb;

export const emptyReducerNoArgs = spacetimedb.reducer({}, ctx => {
});

export const emptyReducerWithInt = spacetimedb.reducer({ count: t.i32() }, (ctx, { count }) => {
});

export const emptyReducerWithString = spacetimedb.reducer({ name: t.string() }, (ctx, { name }) => {
});

export const emptyReducerWithTwoArgs = spacetimedb.reducer({ count: t.i32(), name: t.string() }, (ctx, { count, name }) => {
});

export const emptyReducerWithThreeArgs = spacetimedb.reducer({ active: t.bool(), ratio: t.f32(), label: t.string() }, (ctx, { active, ratio, label }) => {
});
