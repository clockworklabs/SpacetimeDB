import type { U32 } from '../lib/algebraic_type_variants';
import type { Indexes, UniqueIndex } from '../lib/indexes';
import {
  from,
  makeQueryBuilder,
  literal,
  type RowExpr,
  type TableNames,
  type TableSchemaAsTableDef,
} from './query';
import { schema } from '../lib/schema';
import { table, type TableIndexes } from '../lib/table';
import t from '../lib/type_builders';

const person = table(
  {
    name: 'person',
    indexes: [
      {
        name: 'id_name_idx',
        algorithm: 'btree',
        columns: ['id', 'name'] as const,
      },
      {
        name: 'name_idx',
        algorithm: 'btree',
        columns: ['name'] as const,
      },
    ] as const,
  },
  {
    id: t.u32().primaryKey(),
    name: t.string(),
    married: t.bool(),
    id2: t.identity(),
    age: t.u32(),
    age2: t.u16(),
  }
);

const order = table(
  {
    name: 'order',
  },
  {
    order_id: t.u32().primaryKey(),
    item_name: t.string(),
    person_id: t.u32().index(),
  }
);

const spacetimedb = schema([person, order]);

const tableDef = {
  name: person.tableName,
  columns: person.rowType.row,
  indexes: person.idxs, // keep the typed, literal tuples here
} as TableSchemaAsTableDef<typeof person>;

type PersonDef = typeof tableDef;

declare const row: RowExpr<PersonDef>;

const x: U32 = row.age.spacetimeType;

type SchemaTableNames = TableNames<(typeof spacetimedb)['schemaType']>;
const y: SchemaTableNames = 'person';

const orderDef = {
  name: order.tableName,
  columns: order.rowType.row,
  indexes: order.idxs,
};

// spacetimedb.init((ctx: any) => {
//   const qb = makeQueryBuilder(spacetimedb.schemaType);
//   const firstQuery = from(qb.person);
//   firstQuery.semijoinTo(
//     qb.order,
//     p => p.age,
//     o => o.item_name
//   );
//   let item_name = qb.order.cols.item_name;
//   // const filteredQuery = firstQuery.where(row => eq(row.age, literal(20)));
//   // const filteredQuery = firstQuery.where(row => row.age.eq(literal(20)));
//   const filteredQuery = firstQuery.where(row => row.age.eq(literal(20)));

//   // Eventually this should not type check.
//   const _semijoin = filteredQuery.semijoinTo(
//     qb.order,
//     p => p.age,
//     o => o.item_name
//   );
// });

// spacetimedb.init((ctx) => {
//   const qb = makeQueryBuilder(spacetimedb.schemaType);
//   const firstQuery = from(qb.person);
//   firstQuery.semijoinTo(
//     qb.order,
//     p => p.age,
//     o => o.item_name
//   );
//   let item_name = qb.order.cols.item_name;
//   // const filteredQuery = firstQuery.where(row => eq(row.age, literal(20)));
//   // const filteredQuery = firstQuery.where(row => row.age.eq(literal(20)));
//   var filteredQuery = firstQuery.where(row => row.age.eq(literal(20)));
//   filteredQuery = firstQuery.where(row => row.age.eq(row.id));
//   filteredQuery = firstQuery.where(row => row.age.eq(row.id));
//   // filteredQuery = firstQuery.where(row => row.age.eq("yo"));

//   // Eventually this should not type check.
//   // const _semijoin = filteredQuery.semijoinTo(
//   //   qb.order,
//   //   p => p.age,
//   //   o => o.item_name
//   // );
// });
