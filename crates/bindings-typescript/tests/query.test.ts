import { describe, expect, it } from 'vitest';
import { Identity } from '../src/lib/identity';
import {
  makeQueryBuilder,
  eq,
  literal,
  and,
  or,
  not,
} from '../src/server/query';
import type { UntypedSchemaDef } from '../src/server/schema';
import { table } from '../src/server/table';
import { t } from '../src/server/type_builders';

const personTable = table(
  { name: 'person' },
  {
    id: t.identity(),
    name: t.string(),
    age: t.u32(),
  }
);

const schemaDef: UntypedSchemaDef = {
  tables: [
    {
      name: personTable.tableName,
      columns: personTable.rowType.row,
      indexes: personTable.idxs,
    },
  ],
};

describe('TableScan.toSql', () => {
  it('renders a full-table scan when no filters are applied', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = qb.query('person').toSql();

    expect(sql).toBe('SELECT * FROM "person"');
  });

  it('renders a WHERE clause for simple equality filters', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = qb
      .query('person')
      .filter(row => eq(row.name, literal("O'Brian")))
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE "person"."name" = 'O''Brian'`
    );
  });

  it('renders numeric literals and column references', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = qb
      .query('person')
      .filter(row => eq(row.age, literal(42)))
      .toSql();

    expect(sql).toBe(`SELECT * FROM "person" WHERE "person"."age" = 42`);
  });

  it('renders AND clauses across multiple predicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = qb
      .query('person')
      .filter(row =>
        and(eq(row.name, literal('Alice')), eq(row.age, literal(30)))
      )
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE ("person"."name" = 'Alice') AND ("person"."age" = 30)`
    );
  });

  it('renders NOT clauses around subpredicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = qb
      .query('person')
      .filter(row => not(eq(row.name, literal('Bob'))))
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE NOT ("person"."name" = 'Bob')`
    );
  });

  it('renders OR clauses across multiple predicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = qb
      .query('person')
      .filter(row =>
        or(eq(row.name, literal('Carol')), eq(row.name, literal('Dave')))
      )
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE ("person"."name" = 'Carol') OR ("person"."name" = 'Dave')`
    );
  });

  it('renders Identity literals using their hex form', () => {
    const qb = makeQueryBuilder(schemaDef);
    const identity = new Identity(
      '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'
    );
    const sql = qb
      .query('person')
      .filter(row => eq(row.id, literal(identity)))
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE "person"."id" = 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`
    );
  });
});
