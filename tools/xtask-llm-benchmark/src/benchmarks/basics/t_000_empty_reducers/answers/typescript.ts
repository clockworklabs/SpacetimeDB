import { schema, t } from 'spacetimedb/server';

const spacetimedb = schema();

spacetimedb.reducer('emptyReducerNoArgs', {}, ctx => {
});

spacetimedb.reducer('emptyReducerWithInt', { count: t.i32() }, (ctx, { count }) => {
});

spacetimedb.reducer('emptyReducerWithString', { name: t.string() }, (ctx, { name }) => {
});

spacetimedb.reducer('emptyReducerWithTwoArgs', { count: t.i32(), name: t.string() }, (ctx, { count, name }) => {
});

spacetimedb.reducer('emptyReducerWithThreeArgs', { active: t.bool(), ratio: t.f32(), label: t.string() }, (ctx, { active, ratio, label }) => {
});
