import { schema } from '../lib/schema';
import { table } from '../lib/table';
import t from '../lib/type_builders';

const person = table(
  {
    name: 'person',
    indexes: [
      {
        name: 'name_id_idx',
        algorithm: 'btree',
        columns: ['name', 'id'] as const,
      },
    ],
  },
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
  {
    name: 'order',
    indexes: [
      {
        name: 'id_person_id', // We are adding this to make sure `person_id` still isn't considered indexed.
        algorithm: 'btree',
        columns: ['id', 'person_id'] as const,
      },
    ],
  },
  {
    id: t.u32().primaryKey(),
    person_name: t.string().index(),
    person_id: t.u32(),
  }
);

const spacetime = schema(person, order, personWithExtra);

const arrayRetValue = t.array(person.rowType);

spacetime.anonymousView({ name: 'v1', public: true }, arrayRetValue, ctx => {
  type _Expand<T> = T extends infer U ? { [K in keyof U]: U[K] } : never;
  const idk = ctx.from.person.build();
  type Test = _Expand<typeof idk>;
  type T2 = _Expand<(typeof idk)['__algebraicType']>;
  return ctx.from.person.build();
});

spacetime.anonymousView(
  { name: 'v2', public: true },
  arrayRetValue,
  // @ts-expect-error returns a query of the wrong type.
  ctx => {
    type _Expand<T> = T extends infer U ? { [K in keyof U]: U[K] } : never;
    const idk = ctx.from.order.build();
    type Test = _Expand<typeof idk>;

    return ctx.from.order.build();
  }
);

// We should eventually make this fail.
spacetime.anonymousView({ name: 'v3', public: true }, arrayRetValue, ctx => {
  return ctx.from.personWithExtra.build();
});

spacetime.anonymousView({ name: 'v4', public: true }, arrayRetValue, ctx => {
  // @ts-expect-error returns a query of the wrong type.
  const _invalid = ctx.from.person.where(row => row.id.eq('string')).build();
  const _columnEqs = ctx.from.person.where(row => row.id.eq(row.id)).build();
  return ctx.from.person.where(row => row.id.eq(5)).build();
});

spacetime.anonymousView({ name: 'v5', public: true }, arrayRetValue, ctx => {
  const _nonIndexedSemijoin = ctx.from.person
    .where(row => row.id.eq(5))
    // @ts-expect-error person_id is not indexed.
    .leftSemijoin(ctx.from.order, (p, o) => p.id.eq(o.person_id))
    .build();
  const _fromCompositeIndex = ctx.from.person
    .where(row => row.id.eq(5))
    .leftSemijoin(ctx.from.order, (p, o) => p.name.eq(o.person_name))
    .build();
  return ctx.from.person
    .where(row => row.id.eq(5))
    .leftSemijoin(ctx.from.order, (p, o) => p.id.eq(o.id))
    .build();
});
