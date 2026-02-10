import { schema } from './schema';
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

const personWithMissing = table(
  { name: 'personWithMissing' },
  {
    id: t.u32(),
  }
);

const personReordered = table(
  { name: 'personReordered' },
  {
    name: t.string(),
    id: t.u32(),
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

const spacetime = schema(
  person,
  order,
  personWithExtra,
  personReordered,
  personWithMissing
);

const arrayRetValue = t.array(person.rowType);
const optionalPerson = t.option(person.rowType);

spacetime.anonymousView({ name: 'v1', public: true }, arrayRetValue, ctx => {
  return ctx.from.person.build();
});

spacetime.anonymousView(
  { name: 'optionalPerson', public: true },
  optionalPerson,
  ctx => {
    return ctx.db.person.iter().next().value;
  }
);

spacetime.anonymousView(
  { name: 'optionalPersonWrong', public: true },
  optionalPerson,
  // @ts-expect-error returns a value of the wrong type.
  ctx => {
    return ctx.db.order.iter().next().value;
  }
);

// Extra fields are only an issue for queries.
spacetime.anonymousView(
  { name: 'optionalPersonWithExtra', public: true },
  optionalPerson,
  ctx => {
    return ctx.db.personWithExtra.iter().next().value;
  }
);

spacetime.anonymousView(
  { name: 'v2', public: true },
  arrayRetValue,
  // @ts-expect-error returns a query of the wrong type.
  ctx => {
    return ctx.from.order.build();
  }
);

// For queries, we can't return rows with extra fields.
spacetime.anonymousView(
  { name: 'v3', public: true },
  arrayRetValue,
  // @ts-expect-error returns a query of the wrong type.
  ctx => {
    return ctx.from.personWithExtra.build();
  }
);

// Ideally this would fail, since we depend on the field ordering for serialization.
spacetime.anonymousView(
  { name: 'reorderedPerson', public: true },
  arrayRetValue,
  // Comment this out if we can fix the types.
  // // @ts-expect-error returns a query of the wrong type.
  ctx => {
    return ctx.from.personReordered.build();
  }
);

// Fails because it is missing a field.
spacetime.anonymousView(
  { name: 'missingField', public: true },
  arrayRetValue,
  // @ts-expect-error returns a query of the wrong type.
  ctx => {
    return ctx.from.personWithMissing.build();
  }
);

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
