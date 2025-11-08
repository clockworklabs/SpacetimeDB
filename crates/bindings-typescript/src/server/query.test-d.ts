import type { U32 } from '../lib/autogen/algebraic_type_variants';
import type { Indexes, UniqueIndex } from './indexes';
import {
  eq,
  literal,
  type ColumnExpr,
  type RowExpr,
  type TableNames,
  type TableSchemaAsTableDef,
} from './query';
import { schema } from './schema';
import { table, type TableIndexes } from './table';
import t from './type_builders';

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

/*
type PersonDef = {
  name: typeof person.tableName;
  columns: typeof person.rowType.row;
  indexes: typeof person.idxs;
};
*/

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

//idxs2.

spacetimedb.init(ctx => {
  // ctx.db.person.
  //ctx.db.person.
  //ctx.db
  // ctx.db.person.id_name_idx.find

  // Downside of the string approach for columns is that if I hover, I don't get the type information.

  // ctx.queryBuilder.
  // .filter
  // col("age")

  // ctx.db.person[Symbol]

  //ctx.query.from('person')
  ctx.queryBuilder.person
    // .query('person')
    .filter(row => eq(row['age'], literal(20)));
  // .filter(row => eq(row.age, literal(20)))
});
