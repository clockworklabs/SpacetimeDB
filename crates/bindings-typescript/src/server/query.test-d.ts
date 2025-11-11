import type { U32 } from '../lib/autogen/algebraic_type_variants';
import type { Indexes, UniqueIndex } from './indexes';
import {
  eq,
  literal,
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

spacetimedb.init(ctx => {
  const firstQuery = ctx.queryBuilder.query('person');
  firstQuery.semijoinTo(
    ctx.queryBuilder.order,
    p => p.age,
    o => o.item_name
  );
  const filteredQuery = ctx.queryBuilder
    .query('person')
    .filter(row => eq(row.age, literal(20)));

  // Eventually this should not type check.
  const _semijoin = filteredQuery.semijoinTo(
    ctx.queryBuilder.order,
    p => p.age,
    o => o.item_name
  );
});
