import { describe, expect, it } from 'vitest';
import { Identity } from '../src/lib/identity';
import {
  makeQueryBuilder,
  eq,
  literal,
  and,
  or,
  not,
  from,
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

const ordersTable = table(
  { name: 'orders' },
  {
    order_id: t.identity(),
    person_id: t.identity(),
    item_name: t.string(),
  }
);

const schemaDef = {
  tables: [
    {
      name: personTable.tableName,
      columns: personTable.rowType.row,
      indexes: personTable.idxs,
    },
    {
      name: ordersTable.tableName,
      columns: ordersTable.rowType.row,
      indexes: ordersTable.idxs,
    },
  ],
} as const satisfies UntypedSchemaDef;

describe('TableScan.toSql', () => {
  it('renders a full-table scan when no filters are applied', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person).toSql();

    expect(sql).toBe('SELECT * FROM "person"');
  });

  it('renders a WHERE clause for simple equality filters', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => eq(row.name, literal("O'Brian")))
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE "person"."name" = 'O''Brian'`
    );
  });

  it('renders numeric literals and column references', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => eq(row.age, literal(42)))
      .toSql();

    expect(sql).toBe(`SELECT * FROM "person" WHERE "person"."age" = 42`);
  });

  it('renders AND clauses across multiple predicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row =>
        and(eq(row.name, literal('Alice')), eq(row.age, literal(30)))
      )
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE ("person"."name" = 'Alice') AND ("person"."age" = 30)`
    );
  });

  it('renders NOT clauses around subpredicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => not(eq(row.name, literal('Bob'))))
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE NOT ("person"."name" = 'Bob')`
    );
  });

  it('accumulates multiple filters with AND logic', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => eq(row.name, literal('Eve')))
      .where(row => eq(row.age, literal(25)))
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE ("person"."name" = 'Eve') AND ("person"."age" = 25)`
    );
  });

  it('renders OR clauses across multiple predicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row =>
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
    const sql = from(qb.person)
      .where(row => eq(row.id, literal(identity)))
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE "person"."id" = 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`
    );
  });

  it('renders semijoin queries without additional filters', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .semijoinTo(
        qb.orders,
        person => person.id,
        order => order.person_id
      )
      .toSql();

    expect(sql).toBe(
      `SELECT "right".* from "person" "left" join "orders" "right" on "left"."id" = "right"."person_id"`
    );
  });

  it('renders semijoin queries alongside existing predicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => eq(row.age, literal(42)))
      .semijoinTo(
        qb.orders,
        person => person.id,
        order => order.person_id
      )
      .toSql();

    expect(sql).toBe(
      `SELECT "right".* from "person" "left" join "orders" "right" on "left"."id" = "right"."person_id" WHERE "left"."age" = 42`
    );
  });

  it('escapes literals when rendering semijoin filters', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => eq(row.name, literal("O'Brian")))
      .semijoinTo(
        qb.orders,
        person => person.id,
        order => order.person_id
      )
      .toSql();

    expect(sql).toBe(
      `SELECT "right".* from "person" "left" join "orders" "right" on "left"."id" = "right"."person_id" WHERE "left"."name" = 'O''Brian'`
    );
  });

  it('renders compound AND filters for semijoin queries', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row =>
        and(eq(row.name, literal('Alice')), eq(row.age, literal(30)))
      )
      .semijoinTo(
        qb.orders,
        person => person.id,
        order => order.person_id
      )
      .toSql();

    expect(sql).toBe(
      `SELECT "right".* from "person" "left" join "orders" "right" on "left"."id" = "right"."person_id" WHERE ("left"."name" = 'Alice') AND ("left"."age" = 30)`
    );
  });
});
