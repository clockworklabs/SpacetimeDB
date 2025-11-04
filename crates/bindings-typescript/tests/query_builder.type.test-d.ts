import {
  TableScan,
  and,
  createQuery,
  createTableRef,
  eq,
  gt,
  literal,
  lt,
} from '../src/server/query_builder';
import { table, t } from '../src/server';

const person = table(
  {
    name: 'person',
    indexes: [
      {
        name: 'id_name_idx',
        algorithm: 'btree',
        columns: ['id', 'name'],
      },
    ],
  },
  {
    id: t.u32().primaryKey(),
    name: t.string(),
  }
);

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

// @ts-expect-error mismatched types in eq should fail
createQuery(person).filter(row => eq(row.id, literal('oops')));

// @ts-expect-error boolean column cannot be used directly as a filter expression
createQuery(person).filter(row => row.married);

const personRef = createTableRef(person);
const orders = table(
  {
    name: 'orders',
    indexes: [
      {
        name: 'buyer_idx',
        algorithm: 'btree',
        columns: ['buyerId'] as const,
      },
      {
        name: 'id_desc_idx',
        algorithm: 'btree',
        columns: ['id', 'desc'] as const,
      },
    ],
  },
  {
    id: t.u32().primaryKey(),
    buyerId: t.u32().index('btree'),
    desc: t.string(),
  }
);

const ordersRef = createTableRef(orders);
const personScan = new TableScan(personRef);

personScan.semijoin(personRef.indexes.id, ordersRef.indexes.buyer_idx);
// @ts-expect-error mismatched index shapes should fail
personScan.semijoin(personRef.indexes.id, ordersRef.indexes.id_desc_idx);
