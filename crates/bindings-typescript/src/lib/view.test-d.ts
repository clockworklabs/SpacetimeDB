import { schema } from './schema';
import { table } from './table';
import { type ViewFn } from './views';
import t from './type_builders';
import { convert, type RowTypedQuery } from '../server/query';

const person = table(
  { name: 'person' },
  {
    id: t.u32(),
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
    id: t.u32(),
    name2: t.string(),
    person_id: t.u32(),
  }
);

const spacetime = schema(person, order, personWithExtra);

const arrayRetValue = t.array(person.rowType);

spacetime.anonymousView({ name: 'idk', public: true }, arrayRetValue, ctx => {
  return convert(ctx.from.person.build());
});

spacetime.anonymousView(
  { name: 'idk', public: true },
  arrayRetValue,
  // @ts-expect-error returns a query of the wrong type.
  ctx => {
    return convert(ctx.from.order.build());
  }
);

spacetime.anonymousView({ name: 'idk', public: true }, arrayRetValue, ctx => {
  return convert(ctx.from.personWithExtra.build());
});
