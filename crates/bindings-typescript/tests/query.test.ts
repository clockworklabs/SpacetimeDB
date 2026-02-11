import { describe, expect, it } from 'vitest';
import { Identity } from '../src/lib/identity';
import {
  makeQueryBuilder,
  and,
  or,
  not,
  from,
  toSql,
} from '../src/server/query';
import type { UntypedSchemaDef } from '../src/lib/schema';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';

const personTable = table(
  {
    name: 'person',

    indexes: [
      {
        name: 'id_name_idx',
        algorithm: 'btree',
        columns: ['id', 'name'] as const,
      },
    ] as const,
  },
  {
    id: t.identity(),
    name: t.string(),
    age: t.u32(),
  }
);

const ordersTable = table(
  {
    name: 'orders',
    indexes: [
      {
        name: 'orders_person_id_idx',
        algorithm: 'btree',
        columns: ['person_id'],
      },
    ] as const,
  },

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
    const sql = toSql(from(qb.person).build());

    expect(sql).toBe('SELECT * FROM "person"');
  });

  it('renders a WHERE clause for simple equality filters', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .where(row => row.name.eq("O'Brian"))
        .build()
    );

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE "person"."name" = 'O''Brian'`
    );
  });

  it('renders numeric literals and column references', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .where(row => row.age.eq(42))
        .build()
    );

    expect(sql).toBe(`SELECT * FROM "person" WHERE "person"."age" = 42`);
  });

  it('renders AND clauses across multiple predicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .where(row => and(row.name.eq('Alice'), row.age.eq(30)))
        .build()
    );

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE ("person"."name" = 'Alice') AND ("person"."age" = 30)`
    );
  });

  it('renders NOT clauses around subpredicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .where(row => not(row.name.eq('Bob')))
        .build()
    );

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE NOT ("person"."name" = 'Bob')`
    );
  });

  it('accumulates multiple filters with AND logic', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .where(row => row.name.eq('Eve'))
        .where(row => row.age.eq(25))
        .build()
    );

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE ("person"."name" = 'Eve') AND ("person"."age" = 25)`
    );
  });

  it('renders OR clauses across multiple predicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .where(row => or(row.name.eq('Carol'), row.name.eq('Dave')))
        .build()
    );

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE ("person"."name" = 'Carol') OR ("person"."name" = 'Dave')`
    );
  });

  it('renders Identity literals using their hex form', () => {
    const qb = makeQueryBuilder(schemaDef);
    const identity = new Identity(
      '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'
    );
    const sql = toSql(
      from(qb.person)
        .where(row => row.id.eq(identity))
        .build()
    );

    expect(sql).toBe(
      `SELECT * FROM "person" WHERE "person"."id" = 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`
    );
  });

  it('renders semijoin queries without additional filters', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .rightSemijoin(qb.orders, (person, order) =>
          order.person_id.eq(person.id)
        )
        .build()
    );

    expect(sql).toBe(
      `SELECT "orders".* FROM "person" JOIN "orders" ON "orders"."person_id" = "person"."id"`
    );
  });

  it('renders semijoin queries alongside existing predicates', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .where(row => row.age.eq(42))
        .rightSemijoin(qb.orders, (person, order) =>
          order.person_id.eq(person.id)
        )
        .build()
    );

    expect(sql).toBe(
      `SELECT "orders".* FROM "person" JOIN "orders" ON "orders"."person_id" = "person"."id" WHERE "person"."age" = 42`
    );
  });

  it('escapes literals when rendering semijoin filters', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .where(row => row.name.eq("O'Brian"))
        .rightSemijoin(qb.orders, (person, order) =>
          order.person_id.eq(person.id)
        )
        .build()
    );

    expect(sql).toBe(
      `SELECT "orders".* FROM "person" JOIN "orders" ON "orders"."person_id" = "person"."id" WHERE "person"."name" = 'O''Brian'`
    );
  });

  it('renders compound AND filters for semijoin queries', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .where(row => and(row.name.eq('Alice'), row.age.eq(30)))
        .rightSemijoin(qb.orders, (person, order) =>
          order.person_id.eq(person.id)
        )
        .build()
    );

    expect(sql).toBe(
      `SELECT "orders".* FROM "person" JOIN "orders" ON "orders"."person_id" = "person"."id" WHERE ("person"."name" = 'Alice') AND ("person"."age" = 30)`
    );
  });

  it('basic where', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(qb.orders.where(o => o.item_name.eq('Gadget')).build());
    expect(sql).toBe(
      `SELECT * FROM "orders" WHERE "orders"."item_name" = 'Gadget'`
    );
  });

  it('basic where ne', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(qb.orders.where(o => o.item_name.ne('Gadget')).build());
    expect(sql).toBe(
      `SELECT * FROM "orders" WHERE "orders"."item_name" <> 'Gadget'`
    );
  });

  it('basic where lt', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(qb.orders.where(o => o.item_name.lt('Gadget')).build());
    expect(sql).toBe(
      `SELECT * FROM "orders" WHERE "orders"."item_name" < 'Gadget'`
    );
  });

  it('basic where lte', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(qb.orders.where(o => o.item_name.lte('Gadget')).build());
    expect(sql).toBe(
      `SELECT * FROM "orders" WHERE "orders"."item_name" <= 'Gadget'`
    );
  });

  it('basic where gt', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(qb.orders.where(o => o.item_name.gt('Gadget')).build());
    expect(sql).toBe(
      `SELECT * FROM "orders" WHERE "orders"."item_name" > 'Gadget'`
    );
  });

  it('basic where gte', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(qb.orders.where(o => o.item_name.gte('Gadget')).build());
    expect(sql).toBe(
      `SELECT * FROM "orders" WHERE "orders"."item_name" >= 'Gadget'`
    );
  });

  it('basic semijoin', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      qb.person.rightSemijoin(qb.orders, (p, o) => p.id.eq(o.person_id)).build()
    );
    expect(sql).toBe(
      `SELECT "orders".* FROM "person" JOIN "orders" ON "person"."id" = "orders"."person_id"`
    );
  });

  it('basic left semijoin', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      qb.person.leftSemijoin(qb.orders, (p, o) => p.id.eq(o.person_id)).build()
    );
    expect(sql).toBe(
      `SELECT "person".* FROM "orders" JOIN "person" ON "person"."id" = "orders"."person_id"`
    );
  });

  it('method-style chaining with .and()', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .where(row => row.age.gt(20).and(row.age.lt(30)))
        .build()
    );
    expect(sql).toBe(
      `SELECT * FROM "person" WHERE ("person"."age" > 20) AND ("person"."age" < 30)`
    );
  });

  it('method-style chaining with .or()', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .where(row => row.name.eq('Carol').or(row.name.eq('Dave')))
        .build()
    );
    expect(sql).toBe(
      `SELECT * FROM "person" WHERE ("person"."name" = 'Carol') OR ("person"."name" = 'Dave')`
    );
  });

  it('method-style chaining with .not()', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      from(qb.person)
        .where(row => row.name.eq('Bob').not())
        .build()
    );
    expect(sql).toBe(
      `SELECT * FROM "person" WHERE NOT ("person"."name" = 'Bob')`
    );
  });

  it('semijoin with filters on both sides', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(
      qb.person
        .where(row => row.age.eq(42))
        .rightSemijoin(qb.orders, (p, o) => p.id.eq(o.person_id))
        .where(row => row.item_name.eq('Gadget'))
        .build()
    );
    expect(sql).toBe(
      `SELECT "orders".* FROM "person" JOIN "orders" ON "person"."id" = "orders"."person_id" WHERE ("person"."age" = 42) AND ("orders"."item_name" = 'Gadget')`
    );
  });

  it('passes builder directly to toSql() without .build()', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(qb.person.where(row => row.age.eq(42)));
    expect(sql).toBe(`SELECT * FROM "person" WHERE "person"."age" = 42`);
  });

  it('passes table ref directly to toSql() without .build()', () => {
    const qb = makeQueryBuilder(schemaDef);
    const sql = toSql(qb.person);
    expect(sql).toBe('SELECT * FROM "person"');
  });
});
