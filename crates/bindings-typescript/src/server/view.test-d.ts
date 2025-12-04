import { literal } from '.';
import { schema } from '../lib/schema';
import { table } from '../lib/table';
import t from '../lib/type_builders';

const person = table(
  { name: 'person' },
  {
    id: t.u32().primaryKey(),
    name: t.string(),
  }
);

const personWithExtra = table(
  { name: 'personWithExtra' },
  {
    id: t.u32(),
    name: t.string(),
    extraField: t.string(),
  }
);

const order = table(
  { name: 'order' },
  {
    id: t.u32().primaryKey(),
    name2: t.string(),
    person_id: t.u32(),
  }
);

const spacetime = schema(person, order, personWithExtra);

const arrayRetValue = t.array(person.rowType);

spacetime.anonymousView({ name: 'v1', public: true }, arrayRetValue, ctx => {
  return ctx.from.person.build();
});

spacetime.anonymousView(
  { name: 'v2', public: true },
  arrayRetValue,
  // @ts-expect-error returns a query of the wrong type.
  ctx => {
    return ctx.from.order.build();
  }
);

// We should eventually make this fail.
spacetime.anonymousView({ name: 'v3', public: true }, arrayRetValue, ctx => {
  return ctx.from.personWithExtra.build();
});

spacetime.anonymousView({ name: 'v4', public: true }, arrayRetValue, ctx => {
  // @ts-expect-error this uses a literal of the wrong type.
  const x = ctx.from.person.where(row => row.id.eq(literal('string'))).build();
  return ctx.from.person.where(row => row.id.eq(literal(5))).build();
});

spacetime.anonymousView({ name: 'v5', public: true }, arrayRetValue, ctx => {
  return ctx.from.person
    .where(row => row.id.eq(literal(5)))
    .semijoinLeft(ctx.from.order, (p, o) => p.id.eq(o.id))
    .build();
});
