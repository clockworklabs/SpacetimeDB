import { describe, expect, it, assertType } from 'vitest';
import { Identity } from '../src/lib/identity';
import { and, not, or, toSql } from '../src/lib/query';
import { tables } from '../test-app/src/module_bindings';

describe('ClientQuery.toSql', () => {
  it('renders a full-table scan when no filters are applied', () => {
    const sql = toSql(tables.player.build());

    expect(sql).toBe('SELECT * FROM "player"');
  });

  it('renders a WHERE clause for simple equality filters', () => {
    const sql = toSql(
      tables.player.where(row => row.name.eq("O'Brian")).build()
    );

    expect(sql).toBe(
      `SELECT * FROM "player" WHERE "player"."name" = 'O''Brian'`
    );
  });

  it('renders numeric literals and column references', () => {
    const sql = toSql(tables.player.where(row => row.id.eq(42)).build());

    expect(sql).toBe(`SELECT * FROM "player" WHERE "player"."id" = 42`);
  });

  it('renders AND clauses across multiple predicates', () => {
    const sql = toSql(
      tables.player
        .where(row => and(row.name.eq('Alice'), row.id.eq(30)))
        .build()
    );

    expect(sql).toBe(
      `SELECT * FROM "player" WHERE ("player"."name" = 'Alice') AND ("player"."id" = 30)`
    );
  });

  it('renders NOT clauses around subpredicates', () => {
    const sql = toSql(
      tables.player.where(row => not(row.name.eq('Bob'))).build()
    );

    expect(sql).toBe(
      `SELECT * FROM "player" WHERE NOT ("player"."name" = 'Bob')`
    );
  });

  it('accumulates multiple filters with AND logic', () => {
    const sql = toSql(
      tables.player
        .where(row => row.name.eq('Eve'))
        .where(row => row.id.eq(25))
        .build()
    );

    expect(sql).toBe(
      `SELECT * FROM "player" WHERE ("player"."name" = 'Eve') AND ("player"."id" = 25)`
    );
  });

  it('renders OR clauses across multiple predicates', () => {
    const sql = toSql(
      tables.player
        .where(row => or(row.name.eq('Carol'), row.name.eq('Dave')))
        .build()
    );

    expect(sql).toBe(
      `SELECT * FROM "player" WHERE ("player"."name" = 'Carol') OR ("player"."name" = 'Dave')`
    );
  });

  it('renders Identity literals using their hex form', () => {
    const identity = new Identity(
      '0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef'
    );
    const sql = toSql(
      tables.user.where(row => row.identity.eq(identity)).build()
    );

    expect(sql).toBe(
      `SELECT * FROM "user" WHERE "user"."identity" = 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`
    );
  });

  it('renders semijoin queries without additional filters', () => {
    const sql = toSql(
      tables.player
        .rightSemijoin(tables.unindexedPlayer, (player, other) =>
          other.id.eq(player.id)
        )
        .build()
    );

    expect(sql).toBe(
      `SELECT "unindexed_player".* FROM "player" JOIN "unindexed_player" ON "unindexed_player"."id" = "player"."id"`
    );
  });

  it('renders semijoin queries alongside existing predicates', () => {
    const sql = toSql(
      tables.player
        .where(row => row.id.eq(42))
        .rightSemijoin(tables.unindexedPlayer, (player, other) =>
          other.id.eq(player.id)
        )
        .build()
    );

    expect(sql).toBe(
      `SELECT "unindexed_player".* FROM "player" JOIN "unindexed_player" ON "unindexed_player"."id" = "player"."id" WHERE "player"."id" = 42`
    );
  });

  it('escapes literals when rendering semijoin filters', () => {
    const sql = toSql(
      tables.player
        .where(row => row.name.eq("O'Brian"))
        .rightSemijoin(tables.unindexedPlayer, (player, other) =>
          other.id.eq(player.id)
        )
        .build()
    );

    expect(sql).toBe(
      `SELECT "unindexed_player".* FROM "player" JOIN "unindexed_player" ON "unindexed_player"."id" = "player"."id" WHERE "player"."name" = 'O''Brian'`
    );
  });

  it('renders compound AND filters for semijoin queries', () => {
    const sql = toSql(
      tables.player
        .where(row => and(row.name.eq('Alice'), row.id.eq(30)))
        .rightSemijoin(tables.unindexedPlayer, (player, other) =>
          other.id.eq(player.id)
        )
        .build()
    );

    expect(sql).toBe(
      `SELECT "unindexed_player".* FROM "player" JOIN "unindexed_player" ON "unindexed_player"."id" = "player"."id" WHERE ("player"."name" = 'Alice') AND ("player"."id" = 30)`
    );
  });

  it('basic where', () => {
    const sql = toSql(
      tables.player.where(row => row.name.eq('Gadget')).build()
    );
    expect(sql).toBe(`SELECT * FROM "player" WHERE "player"."name" = 'Gadget'`);
  });

  it('basic where ne', () => {
    const sql = toSql(
      tables.player.where(row => row.name.ne('Gadget')).build()
    );
    expect(sql).toBe(
      `SELECT * FROM "player" WHERE "player"."name" <> 'Gadget'`
    );
  });

  it('basic where lt', () => {
    const sql = toSql(
      tables.player.where(row => row.name.lt('Gadget')).build()
    );
    expect(sql).toBe(`SELECT * FROM "player" WHERE "player"."name" < 'Gadget'`);
  });

  it('basic where lte', () => {
    const sql = toSql(
      tables.player.where(row => row.name.lte('Gadget')).build()
    );
    expect(sql).toBe(
      `SELECT * FROM "player" WHERE "player"."name" <= 'Gadget'`
    );
  });

  it('basic where gt', () => {
    const sql = toSql(
      tables.player.where(row => row.name.gt('Gadget')).build()
    );
    expect(sql).toBe(`SELECT * FROM "player" WHERE "player"."name" > 'Gadget'`);
  });

  it('basic where gte', () => {
    const sql = toSql(
      tables.player.where(row => row.name.gte('Gadget')).build()
    );
    expect(sql).toBe(
      `SELECT * FROM "player" WHERE "player"."name" >= 'Gadget'`
    );
  });

  it('basic semijoin', () => {
    const sql = toSql(
      tables.player
        .rightSemijoin(tables.unindexedPlayer, (player, other) =>
          other.id.eq(player.id)
        )
        .build()
    );
    expect(sql).toBe(
      `SELECT "unindexed_player".* FROM "player" JOIN "unindexed_player" ON "unindexed_player"."id" = "player"."id"`
    );
  });

  it('basic left semijoin', () => {
    const sql = toSql(
      tables.player
        .leftSemijoin(tables.unindexedPlayer, (player, other) =>
          other.id.eq(player.id)
        )
        .build()
    );
    expect(sql).toBe(
      `SELECT "player".* FROM "unindexed_player" JOIN "player" ON "unindexed_player"."id" = "player"."id"`
    );
  });

  it('method-style chaining with .and()', () => {
    const sql = toSql(
      tables.player.where(row => row.id.gt(20).and(row.id.lt(30))).build()
    );
    expect(sql).toBe(
      `SELECT * FROM "player" WHERE ("player"."id" > 20) AND ("player"."id" < 30)`
    );
  });

  it('method-style chaining with .or()', () => {
    const sql = toSql(
      tables.player
        .where(row => row.name.eq('Carol').or(row.name.eq('Dave')))
        .build()
    );
    expect(sql).toBe(
      `SELECT * FROM "player" WHERE ("player"."name" = 'Carol') OR ("player"."name" = 'Dave')`
    );
  });

  it('method-style chaining with .not()', () => {
    const sql = toSql(
      tables.player.where(row => row.name.eq('Bob').not()).build()
    );
    expect(sql).toBe(
      `SELECT * FROM "player" WHERE NOT ("player"."name" = 'Bob')`
    );
  });

  it('semijoin with filters on both sides', () => {
    const sql = toSql(
      tables.player
        .where(row => row.id.eq(42))
        .rightSemijoin(tables.unindexedPlayer, (player, other) =>
          other.id.eq(player.id)
        )
        .where(row => row.name.eq('Gadget'))
        .build()
    );
    expect(sql).toBe(
      `SELECT "unindexed_player".* FROM "player" JOIN "unindexed_player" ON "unindexed_player"."id" = "player"."id" WHERE ("player"."id" = 42) AND ("unindexed_player"."name" = 'Gadget')`
    );
  });
});

// Type-level tests: verify query builder expressions expose toSql() and are
// compatible with useTable's parameter type ({ toSql(): string } & Record<string, any>).
// These use assertType which causes a *compile-time* failure if the types are wrong.
describe('useTable type compatibility', () => {
  type UseTableQuery = { toSql(): string } & Record<string, any>;

  it('table ref (bare table) is assignable to useTable query param', () => {
    assertType<UseTableQuery>(tables.player);
  });

  it('.where() result is assignable to useTable query param', () => {
    assertType<UseTableQuery>(tables.player.where(r => r.name.eq('Hello')));
  });

  it('chained .where() result is assignable to useTable query param', () => {
    assertType<UseTableQuery>(
      tables.player.where(r => r.name.eq('Hello')).where(r => r.id.eq(1))
    );
  });

  it('rightSemijoin result is assignable to useTable query param', () => {
    assertType<UseTableQuery>(
      tables.player.rightSemijoin(tables.unindexedPlayer, (p, o) =>
        o.id.eq(p.id)
      )
    );
  });

  it('leftSemijoin result is assignable to useTable query param', () => {
    assertType<UseTableQuery>(
      tables.player.leftSemijoin(tables.unindexedPlayer, (p, o) =>
        o.id.eq(p.id)
      )
    );
  });

  it('semijoin with .where() is assignable to useTable query param', () => {
    assertType<UseTableQuery>(
      tables.player
        .rightSemijoin(tables.unindexedPlayer, (p, o) => o.id.eq(p.id))
        .where(r => r.name.eq('test'))
    );
  });

  it('table ref exposes toSql() returning string', () => {
    const sql: string = tables.player.toSql();
    expect(typeof sql).toBe('string');
  });

  it('.where() result exposes toSql() returning string', () => {
    const sql: string = tables.player.where(r => r.name.eq('x')).toSql();
    expect(typeof sql).toBe('string');
  });

  it('semijoin result exposes toSql() returning string', () => {
    const sql: string = tables.player
      .rightSemijoin(tables.unindexedPlayer, (p, o) => o.id.eq(p.id))
      .toSql();
    expect(typeof sql).toBe('string');
  });
});
