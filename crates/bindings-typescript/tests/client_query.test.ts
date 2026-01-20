import { describe, expect, it } from 'vitest';
import { Identity } from '../src/lib/identity';
import { and, not, or, toSql } from '../src/lib/query';
import { query } from '../test-app/src/module_bindings';

describe('ClientQuery.toSql', () => {
  it('renders a full-table scan when no filters are applied', () => {
    const sql = toSql(query.player.build());

    expect(sql).toBe('SELECT * FROM "player"');
  });

  it('renders a WHERE clause for simple equality filters', () => {
    const sql = toSql(
      query.player.where(row => row.name.eq("O'Brian")).build()
    );

    expect(sql).toBe(
      `SELECT * FROM "player" WHERE "player"."name" = 'O''Brian'`
    );
  });

  it('renders numeric literals and column references', () => {
    const sql = toSql(query.player.where(row => row.id.eq(42)).build());

    expect(sql).toBe(`SELECT * FROM "player" WHERE "player"."id" = 42`);
  });

  it('renders AND clauses across multiple predicates', () => {
    const sql = toSql(
      query.player
        .where(row => and(row.name.eq('Alice'), row.id.eq(30)))
        .build()
    );

    expect(sql).toBe(
      `SELECT * FROM "player" WHERE ("player"."name" = 'Alice') AND ("player"."id" = 30)`
    );
  });

  it('renders NOT clauses around subpredicates', () => {
    const sql = toSql(
      query.player.where(row => not(row.name.eq('Bob'))).build()
    );

    expect(sql).toBe(
      `SELECT * FROM "player" WHERE NOT ("player"."name" = 'Bob')`
    );
  });

  it('accumulates multiple filters with AND logic', () => {
    const sql = toSql(
      query.player
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
      query.player
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
      query.user.where(row => row.identity.eq(identity)).build()
    );

    expect(sql).toBe(
      `SELECT * FROM "user" WHERE "user"."identity" = 0x0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef`
    );
  });

  it('renders semijoin queries without additional filters', () => {
    const sql = toSql(
      query.player
        .rightSemijoin(query.unindexed_player, (player, other) =>
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
      query.player
        .where(row => row.id.eq(42))
        .rightSemijoin(query.unindexed_player, (player, other) =>
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
      query.player
        .where(row => row.name.eq("O'Brian"))
        .rightSemijoin(query.unindexed_player, (player, other) =>
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
      query.player
        .where(row => and(row.name.eq('Alice'), row.id.eq(30)))
        .rightSemijoin(query.unindexed_player, (player, other) =>
          other.id.eq(player.id)
        )
        .build()
    );

    expect(sql).toBe(
      `SELECT "unindexed_player".* FROM "player" JOIN "unindexed_player" ON "unindexed_player"."id" = "player"."id" WHERE ("player"."name" = 'Alice') AND ("player"."id" = 30)`
    );
  });

  it('basic where', () => {
    const sql = toSql(query.player.where(row => row.name.eq('Gadget')).build());
    expect(sql).toBe(`SELECT * FROM "player" WHERE "player"."name" = 'Gadget'`);
  });

  it('basic where lt', () => {
    const sql = toSql(query.player.where(row => row.name.lt('Gadget')).build());
    expect(sql).toBe(`SELECT * FROM "player" WHERE "player"."name" < 'Gadget'`);
  });

  it('basic where lte', () => {
    const sql = toSql(
      query.player.where(row => row.name.lte('Gadget')).build()
    );
    expect(sql).toBe(
      `SELECT * FROM "player" WHERE "player"."name" <= 'Gadget'`
    );
  });

  it('basic where gt', () => {
    const sql = toSql(query.player.where(row => row.name.gt('Gadget')).build());
    expect(sql).toBe(`SELECT * FROM "player" WHERE "player"."name" > 'Gadget'`);
  });

  it('basic where gte', () => {
    const sql = toSql(
      query.player.where(row => row.name.gte('Gadget')).build()
    );
    expect(sql).toBe(
      `SELECT * FROM "player" WHERE "player"."name" >= 'Gadget'`
    );
  });

  it('basic semijoin', () => {
    const sql = toSql(
      query.player
        .rightSemijoin(query.unindexed_player, (player, other) =>
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
      query.player
        .leftSemijoin(query.unindexed_player, (player, other) =>
          other.id.eq(player.id)
        )
        .build()
    );
    expect(sql).toBe(
      `SELECT "player".* FROM "unindexed_player" JOIN "player" ON "unindexed_player"."id" = "player"."id"`
    );
  });

  it('semijoin with filters on both sides', () => {
    const sql = toSql(
      query.player
        .where(row => row.id.eq(42))
        .rightSemijoin(query.unindexed_player, (player, other) =>
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
