import { describe, expect, test } from 'vitest';
import { and, createQuery, eq, gt, literal, lt } from '../src/server/query_builder';
import type { UntypedTableDef } from '../src/server/table';

describe('QueryBuilder', () => {
  const tableDef: UntypedTableDef = {
    name: 'person',
    columns: {
      id: {} as any,
      name: {} as any,
      age: {} as any,
    },
    indexes: [],
  };

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
      .filter(row =>
        and(eq(row.id, literal(1)), gt(row.age, literal(18)))
      )
      .toSql();

    expect(sql).toBe(
      'SELECT "person".* FROM "person" WHERE ("person"."id" = 1) AND ("person"."age" > 18)'
    );
  });
});
