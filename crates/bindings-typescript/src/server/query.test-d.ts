import type { U32 } from '../lib/autogen/algebraic_type_variants';
import type { Indexes, UniqueIndex } from './indexes';
import {
  eq,
  on,
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


  //ctx.query.from('person')
  ctx.queryBuilder
    .query('person')
    // .query('person')
    .filter(row => eq(row['age'], literal(20)))
    // .filter(row => eq(row.age, literal(20)))
    .join('order', {
      leftColumns: ['age'],
      rightColumns: ['person_id'],
    });

  ctx.queryBuilder
    .query('person')
    .filter(row => eq(row.age, literal(20)))
    .join('order', {
      leftColumns: ['id'],
      rightColumns: ['person_id'],
    });

  ctx.queryBuilder
    .query('person')
    .filter(row => eq(row.age, literal(20)))
    .join4('order', {
     leftColumns: p => [p.id, p.age] as const,
     rightColumns: o => [o.person_id] as const,
    });

  // ctx.queryBuilder
  //   .query('person')
  //   .filter(row => eq(row.age, literal(20)))
  //   .join5('order', {
  //    leftColumns: p => [p.id],
  //    rightColumns: o => [o.person_id],
  //   });
  ctx.queryBuilder
    .query('person')
    .filter(row => eq(row.age, literal(20)))
    .join5('order', {
     leftColumns: ["id"] as const,
     rightColumns: ["person_id", "item_name"] as const
    });

  ctx.queryBuilder
    .query('person')
    .filter(row => eq(row.age, literal(20)))
    .join7('order', {
     leftColumns: ["id"] as const,
     rightColumns: ["person_id", "item_name"] as const
    });

  ctx.queryBuilder
    .query('person')
    .filter(row => eq(row.age, literal(20)))
    .join8('order', {
     leftColumns: ["id"] as const,
     rightColumns: ["person_id"] as const,
     // rightColumns: ["person_id", "item_name"] as const,
    });

  ctx.queryBuilder
    .query('person')
    .filter(row => eq(row.age, literal(20)))
    .join9('order', {
     leftColumns: ["id"] as const,
     rightColumns: ["person_id", "item_name"] as const,
    });

  ctx.queryBuilder
    .query('person')
    .filter(row => eq(row.age, literal(20)))
    .join9('order', on(["id", "name"], ["person_id"])
    );


  // const aQuery = ctx.queryBuilder.query('person')
  //   .filter(row => eq(row.age, literal(20)))
  // // Get the context from somewhere else.

  // let q = ctx.query["foobar"]
  //       .join(ctx.orders, row => eq(row.id, ctx.orders.user_id))
  //       // .join(row => eq(row.id, ctx.orders.user_id))
  
  // ctx.query['foobar']
  //   .filter(row => eq(row.id, literal(5)))
  //   .join(ctx.query.bar.filter(row => eq(row.id, 20)), (left, right) => eq(left.id, right.id))

  // ctx.queryBuilder.foo
  // ctx.query['foobar']
  //   .filter(row => eq(row.id, literal(5)))
  //   // .join(ts => ts.bar)
  //   // .join(|x| x.table)
  //   .join(ctx.)
  //   // .join(ctx.bar, (left, right) => eq(left.id, right.id))


  // ctx.queryBuilder.query('person')
  //   .filter(row => eq(row.age, literal(20)))
  //   .join(aQuery);
  //   //TableName | Query
  // ctx.queryBuilder.query('person').join(ctx.query.person)

  // ctx.queryBuilder.query('person').join(ts => ts.query, {

  // })


  // /*
  // .join((left, right) => eq(left.id, right.person_id))
  // ctx.query.myLeftTable.join(ctx.query.myRightTable)
  // */
  // // ctx.queryBuilder.query('person').join(_.query)
  // // ctx.queryBuilder.query('person').join((tables) => tables.query)
  // // ctx.queryBuilder.query('person').join()
  // ctx.queryBuilder.query('person').join2('order', {
  //   leftColumns: p => [p.id],
  //   rightColumns: o => [o.person_id],
  // });

  // /*
  // ctx.queryBuilder.query('person').join3('order', {
  //   leftColumns: p => [p.id],
  //   rightColumns: o => [o.person_id, o.item_name],
  // });
  // */
 });
