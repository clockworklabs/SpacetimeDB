import { describe, expect, test } from 'vitest';
import {
  and,
  createQuery,
  createTableRef,
  createTableScan,
  TableScan,
  eq,
  gt,
  literal,
  lt,
} from '../src/server/query_builder';
import type { Semijoin } from '../src/server/query_builder';
import type { UntypedTableDef } from '../src/server/table';

import { table, t } from '../src/server';
describe('QueryBuilder', () => {
  const myObjectType = t.object('myObjectType', {
    f1: t.bool(),
    f2: t.string(),
  });
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
      myObject: myObjectType,
      // maybeThing: t.option()
      age: t.u32(),
      age2: t.u16(),
    }
  );

  const tableRef = createTableRef(person);
  const tableDef: UntypedTableDef = tableRef.tableDef;

  test('produces SQL for numeric equality filters', () => {
    const sql = createQuery(tableDef)
      .filter(row => eq(row.id, literal(42)))
      .toSql();

    expect(sql).toBe(
      'SELECT "person".* FROM "person" WHERE "person"."id" = 42'
    );
  });

  test('produces SQL for string equality filters', () => {
    const sql = createQuery(tableDef)
      .filter(row => eq(row.name, literal("O'Malley")))
      .toSql();

    expect(sql).toBe(
      `SELECT "person".* FROM "person" WHERE "person"."name" = 'O''Malley'`
    );
  });

  test('produces SQL for null equality filters', () => {
    const sql = createQuery(tableDef)
      .filter(row => eq(row.name, literal(null)))
      .toSql();

    expect(sql).toBe(
      'SELECT "person".* FROM "person" WHERE "person"."name" IS NULL'
    );
  });

  test('produces SQL for greater-than comparisons', () => {
    const sql = createQuery(tableDef)
      .filter(row => gt(row.age, literal(21)))
      .toSql();

    expect(sql).toBe(
      'SELECT "person".* FROM "person" WHERE "person"."age" > 21'
    );
  });

  test('produces SQL for less-than comparisons', () => {
    const sql = createQuery(tableDef)
      .filter(row => lt(row.age, literal(65)))
      .toSql();

    expect(sql).toBe(
      'SELECT "person".* FROM "person" WHERE "person"."age" < 65'
    );
  });

  test('combines boolean expressions with AND', () => {
    const sql = createQuery(tableDef)
      .filter(row => and(eq(row.id, literal(1)), gt(row.age, literal(18))))
      .toSql();

    expect(sql).toBe(
      'SELECT "person".* FROM "person" WHERE ("person"."id" = 1) AND ("person"."age" > 18)'
    );
  });

  test('filter with object fields', () => {
    const scan = new TableScan(tableRef)
      // .addFilter(row => eq(row.myObject, row.myObject))
      .addFilter(row => eq(row.married, literal(true)))
      .addFilter(row => eq(row.id, literal(1)))
      .addFilter(row => gt(row.age, literal(18)))
      .addFilter(row => gt(row.age, row.age2));

    expect(scan.toSql()).toBe(
      'SELECT "person".* FROM "person" WHERE ("person"."married" = TRUE) AND ("person"."id" = 1) AND ("person"."age" > 18) AND ("person"."age" > "person"."age2")'
    );
  });

  test('table scan renders same SQL as query builder', () => {
    const scan = new TableScan(tableRef)
      .addFilter(row => eq(row.id, literal(5)))
      .addFilter(row => gt(row.age, literal(30)));

    expect(scan.toSql()).toBe(
      'SELECT "person".* FROM "person" WHERE ("person"."id" = 5) AND ("person"."age" > 30)'
    );
  });

  test('table scan filter on bool field', () => {
    const scan = new TableScan(tableRef)
      .addFilter(row => eq(row.id, literal(5)))
      .addFilter(row => eq(row.married, literal(true)));

    expect(scan.toSql()).toBe(
      'SELECT "person".* FROM "person" WHERE ("person"."id" = 5) AND ("person"."married" = TRUE)'
    );
  });

  test('semijoin enforces matching index shapes', () => {
    const orders = table(
      {
        name: 'orders',
        indexes: [
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
          {
            name: 'string_string_idx',
            algorithm: 'btree',
            columns: ['s2', 'desc'] as const,
          },
        ] as const,
      },
      {
        id: t.u32().primaryKey(),
        buyerId: t.u32().index('btree'),
        u16field: t.u16(),
        desc: t.string(),
        s2: t.string(),
      }
    );
    const ordersRef = createTableRef(orders);
    const personScan = new TableScan(tableRef);
    expect(Object.keys(tableRef.indexes)).toContain('id_name_idx');
    expect(tableRef.indexes.id.columns).toEqual(['id']);
    expect(ordersRef.indexes.buyerId.columns).toEqual(['buyerId']);
    const join = new TableScan(tableRef).semijoin(
      tableRef.indexes.id,
      ordersRef.indexes.buyerId
    );
    const join2 = personScan.semijoin(
      tableRef.indexes.id_name_idx,
      ordersRef.indexes.id_desc_idx
    );

    /*
    const _typeCheck: Semijoin<
      typeof tableRef.tableDef,
      typeof ordersRef.tableDef,
      'id',
      'buyerId'
    > = join;
     */

    expect(join.leftIndex.columns).toEqual(['id']);
    expect(join.rightIndex.columns).toEqual(['buyerId']);
    expect(join.rightIndex.table).toBe('orders');
    expect(join2.leftIndex.columns).toEqual(['id', 'name']);
    expect(join2.rightIndex.columns).toEqual(['id', 'desc']);
    expect(Array.isArray(join2.leftIndex.valueType)).toBe(true);
    expect(Array.isArray(join2.rightIndex.valueType)).toBe(true);
    expect(join2.leftIndex.valueType).toHaveLength(2);
    expect(join2.rightIndex.valueType).toHaveLength(2);
    expect(join.toSql()).toBe(
      'SELECT "person".* FROM "person" WHERE EXISTS (SELECT 1 FROM "orders" WHERE ("person"."id" = "orders"."buyerId"))'
    );
    expect(join2.toSql()).toBe(
      'SELECT "person".* FROM "person" WHERE EXISTS (SELECT 1 FROM "orders" WHERE ("person"."id" = "orders"."id") AND ("person"."name" = "orders"."desc"))'
    );
  });
});
