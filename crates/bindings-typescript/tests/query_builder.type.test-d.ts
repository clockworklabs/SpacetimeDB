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

// @ts-expect-error columns with differing spacetime types cannot be compared
createQuery(person).filter(row => eq(row.id, row.age2));

// @ts-expect-error columns with differing spacetime types cannot be compared
createQuery(person).filter(row => eq(row.id, row.age2));

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
      {
        name: 'u16_desc_idx',
        algorithm: 'btree',
        columns: ['u16field', 'desc'] as const,
      },
    ],
  },
  {
    id: t.u32().primaryKey(),
    buyerId: t.u32().index('btree'),
    u16field: t.u16(),
    desc: t.string(),
  }
);

const ordersRef = createTableRef(orders);
const personScan = new TableScan(personRef);

personScan.semijoin(personRef.indexes.id, ordersRef.indexes.buyer_idx);
// @ts-expect-error mismatched index shapes should fail
personScan.semijoin(personRef.indexes.id, ordersRef.indexes.id_desc_idx);
// @ts-expect-error mismatched spacetime types should fail
personScan.semijoin(
  personRef.indexes.id_name_idx,
  ordersRef.indexes.u16_desc_idx
);

const _spacetimeMismatch: typeof personRef.indexes.id_name_idx['valueInfo'] =
  // @ts-expect-error spacetime metadata differs
  ordersRef.indexes.u16_desc_idx.valueInfo;

const _tagOk: typeof personRef.indexes.id_name_idx['valueInfo'][0]['spacetimeTag'] =
  'U32';
// @ts-expect-error tag mismatch
const _tagBad: typeof personRef.indexes.id_name_idx['valueInfo'][0]['spacetimeTag'] =
  'U16';

const personIdTag: ColumnSpacetimeTag<
  typeof tableRef.tableDef,
  'id'
> = 'U32';
// @ts-expect-error tag mismatch
const personIdTagBad: ColumnSpacetimeTag<
  typeof tableRef.tableDef,
  'id'
> = 'U16';

type PersonIdSpacetime = ColumnSpacetimeType<
  typeof tableRef.tableDef,
  'id'
>;
type OrdersU16Spacetime = ColumnSpacetimeType<
  typeof ordersRef.tableDef,
  'u16field'
>;
// @ts-expect-error spacetime types differ (u32 vs u16)
const _spacetimeAssign: PersonIdSpacetime =
  ordersRef.tableDef.columns.u16field.typeBuilder.algebraicType;
