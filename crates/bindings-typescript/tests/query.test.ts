import { describe, expect, it } from 'vitest';
import { Identity } from '../src/lib/identity';
import {
  makeQueryBuilder,
  literal,
  and,
  or,
  not,
  from,
} from '../src/server/query';
import type { UntypedSchemaDef } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';

const personTable = table(
  { name: 'person' },
  {
    id: t.identity(),
    name: t.string(),
    age: t.u32(),
  },
  [
    {
      name: 'person_id_idx',
      algorithm: 'btree',
      columns: ['id'],
    },
  ]
);

const ordersTable = table(
  { name: 'orders' },
  {
    order_id: t.identity(),
    person_id: t.identity(),
    item_name: t.string(),
  },
  [
    {
      name: 'orders_person_id_idx',
      algorithm: 'btree',
      columns: ['person_id'],
    },
  ]
);

const schemaDef = {
  tables: [
    {
      name: personTable.tableName,
      accessorName: personTable.tableName,
      columns: personTable.rowType.row,
      rowType: personTable.rowSpacetimeType,
      indexes: personTable.idxs,
      constraints: personTable.constraints,
    },
    {
      name: ordersTable.tableName,
      accessorName: ordersTable.tableName,
      columns: ordersTable.rowType.row,
      rowType: ordersTable.rowSpacetimeType,
      indexes: ordersTable.idxs,
      constraints: ordersTable.constraints,
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
      .where(row => row.name.eq(literal("O'Brian")))
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE "person"."name" = 'O''Brian'`
    );
  });

  it('renders numeric literals and column references', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => row.age.eq(literal(42)))
      .toSql();

    expect(sql).toBe(`SELECT * FROM "person" WHERE "person"."age" = 42`);
  });

  it('renders AND clauses across multiple predicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => and(row.name.eq(literal('Alice')), row.age.eq(literal(30))))
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE ("person"."name" = 'Alice') AND ("person"."age" = 30)`
    );
  });

  it('renders NOT clauses around subpredicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => not(row.name.eq(literal('Bob'))))
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE NOT ("person"."name" = 'Bob')`
    );
  });

  it('accumulates multiple filters with AND logic', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => row.name.eq(literal('Eve')))
      .where(row => row.age.eq(literal(25)))
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE ("person"."name" = 'Eve') AND ("person"."age" = 25)`
    );
  });

  it('renders OR clauses across multiple predicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row =>
        or(row.name.eq(literal('Carol')), row.name.eq(literal('Dave')))
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
      .where(row => row.id.eq(literal(identity)))
      .toSql();

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE "person"."id" = 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`
    );
  });

  it('renders semijoin queries without additional filters', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .semijoinRight(qb.orders, (person, order) =>
        order.person_id.eq(person.id)
      )
      .toSql();

    expect(sql).toBe(
      `SELECT "orders".* FROM "person" JOIN "orders" ON "orders"."person_id" = "person"."id"`
    );
  });

  it('renders semijoin queries alongside existing predicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => row.age.eq(literal(42)))
      .semijoinRight(qb.orders, (person, order) =>
        order.person_id.eq(person.id)
      )
      .toSql();

    expect(sql).toBe(
      `SELECT "orders".* FROM "person" JOIN "orders" ON "orders"."person_id" = "person"."id" WHERE "person"."age" = 42`
    );
  });

  it('escapes literals when rendering semijoin filters', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => row.name.eq(literal("O'Brian")))
      .semijoinRight(qb.orders, (person, order) =>
        order.person_id.eq(person.id)
      )
      .toSql();

    expect(sql).toBe(
      `SELECT "orders".* FROM "person" JOIN "orders" ON "orders"."person_id" = "person"."id" WHERE "person"."name" = 'O''Brian'`
    );
  });

  it('renders compound AND filters for semijoin queries', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = from(qb.person)
      .where(row => and(row.name.eq(literal('Alice')), row.age.eq(literal(30))))
      .semijoinRight(qb.orders, (person, order) =>
        order.person_id.eq(person.id)
      )
      .toSql();

    expect(sql).toBe(
      `SELECT "orders".* FROM "person" JOIN "orders" ON "orders"."person_id" = "person"."id" WHERE ("person"."name" = 'Alice') AND ("person"."age" = 30)`
    );
  });

  it('basic where', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = qb.orders.where(o => o.item_name.eq(literal('Gadget'))).toSql();
    expect(sql).toBe(
      `SELECT * FROM "orders" WHERE "orders"."item_name" = 'Gadget'`
    );
  });

  it('basic semijoin', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = qb.person
      .semijoinRight(qb.orders, (p, o) => p.id.eq(o.person_id))
      .toSql();
    expect(sql).toBe(
      `SELECT "orders".* FROM "person" JOIN "orders" ON "person"."id" = "orders"."person_id"`
    );
  });

  it('basic left semijoin', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = qb.person
      .semijoinLeft(qb.orders, (p, o) => p.id.eq(o.person_id))
      .toSql();
    expect(sql).toBe(
      `SELECT "person".* FROM "orders" JOIN "person" ON "person"."id" = "orders"."person_id"`
    );
  });

  it('semijoin with filters on both sides', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = qb.person
      .where(row => row.age.eq(literal(42)))
      .semijoinRight(qb.orders, (p, o) => p.id.eq(o.person_id))
      .where(row => row.item_name.eq(literal('Gadget')))
      .toSql();
    expect(sql).toBe(
      `SELECT "orders".* FROM "person" JOIN "orders" ON "person"."id" = "orders"."person_id" WHERE ("person"."age" = 42) AND ("orders"."item_name" = 'Gadget')`
    );
  });
});
