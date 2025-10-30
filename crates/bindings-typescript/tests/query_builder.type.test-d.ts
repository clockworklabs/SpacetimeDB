import { and, createQuery, eq, gt, literal, lt } from '../src/server/query_builder';
import { table, t } from '../src/server';

const person = table({ name: 'person' }, {
  id: t.u32(),
  name: t.string(),
});

createQuery(person).filter(row => {
  row.id;
  row.name;
  // @ts-expect-error accessing unknown field should fail type-check
  row.missing;
  return eq(row.id, literal(1));
});

createQuery(person).filter(row => {
  const comparison = gt(row.id, literal(5));
  const chained = and(comparison, lt(row.id, literal(10)));
  return chained;
});

createQuery(person).filter(row => {
  // @ts-expect-error wrong value type for numeric column
  return gt(row.id, literal('oops'));
});
