import { ScheduleAt } from '../../../crates/bindings-typescript/src';
import { schema, table, t, type Infer, type InferTypeOfRow } from '../../../crates/bindings-typescript/src/server';

// ─────────────────────────────────────────────────────────────────────────────
// SUPPORT TYPES (SpacetimeType equivalents)
// ─────────────────────────────────────────────────────────────────────────────

// Rust: #[derive(SpacetimeType)] pub struct TestB { foo: String }
export const testB = t.object({
  foo: t.string(),
});
export type TestB = Infer<typeof testB>;

// Rust: #[derive(SpacetimeType)] #[sats(name = "Namespace.TestC")] enum TestC { Foo, Bar }
// TODO: support enum name attribute in TS bindings
export const testC = t.enum({
  Foo: t.unit(),
  Bar: t.unit(),
});
export type TestC = Infer<typeof testC>;

const DEFAULT_TEST_C: TestC = { tag: 'Foo', value: {} } as const;

export const testD = {
  testC: testC.default(DEFAULT_TEST_C),
};
export type TestD = InferTypeOfRow<typeof testD>;

// Rust: #[derive(SpacetimeType)] pub struct Baz { pub field: String }
export const Baz = t.object({
  field: t.string(),
});
export type Baz = Infer<typeof Baz>;

// Rust: #[derive(SpacetimeType)] pub enum Foobar { Baz(Baz), Bar, Har(u32) }
export const foobar = t.enum({
  Baz: Baz,
  Bar: t.unit(),
  Har: t.u32(),
});
export type Foobar = Infer<typeof foobar>;

// Rust: #[derive(SpacetimeType)] #[sats(name = "Namespace.TestF")] enum TestF { Foo, Bar, Baz(String) }
// TODO: support enum name attribute in TS bindings
export const TestF = t.enum({
  Foo: t.unit(),
  Bar: t.unit(),
  Baz: t.string(),
});
export type TestF = Infer<typeof TestF>;

// Rust: pub type TestAlias = TestA;
// In TS we’ll just reuse TestA’s row type (see below) where needed.

// Rust: #[derive(Deserialize)] pub struct Foo<'a> { pub field: &'a str }
// In TS we simply model as a struct and provide a BSATN deserializer placeholder.
export const Foo = t.object({ field: t.string() });
export type Foo = Infer<typeof Foo>;
export function Foo_baz_bsatn(bytes: Uint8Array): Foo {
  // If your bindings expose a bsatn decode helper, use it here.
  // return bsatn.fromSlice(bytes, Foo);
  throw new Error('Implement BSATN decode for Foo if needed');
}

// ─────────────────────────────────────────────────────────────────────────────
// SCHEMA AND TABLES
// ─────────────────────────────────────────────────────────────────────────────

export const spacetimedb = schema(
  // person (public) with btree index on age
  table(
    {
      name: 'person',
      public: true,
      indexes: [{ name: 'age', algorithm: 'btree', columns: ['age'] }],
    },
    {
      id: t.u32().primaryKey().autoInc(),
      name: t.string(),
      age: t.u8(),
    }
  ),

  // test_a with index foo on x
  table(
    {
      name: 'test_a',
      indexes: [{ name: 'foo', algorithm: 'btree', columns: ['x'] }],
    },
    {
      x: t.u32(),
      y: t.u32(),
      z: t.string(),
    }
  ),

  // test_d (public) with default(Some(DEFAULT_TEST_C)) option field
  table(
    {
      name: 'test_d',
      public: true,
    },
    {
      test_c: t.option(testC).default(DEFAULT_TEST_C),
      test_c_a: testC.optional().default(DEFAULT_TEST_C), // alternate syntax
    }
  ),

  // test_e, default private, with primary key id auto_inc and btree index on name
  table(
    {
      name: 'test_e',
      public: false,
      indexes: [{ name: 'name', algorithm: 'btree', columns: ['name'] }],
    },
    {
      id: t.u64().primaryKey().autoInc(),
      name: t.string(),
    }
  ),

  // test_f (public) with Foobar field
  table(
    { name: 'test_f', public: true },
    {
      field: foobar,
    }
  ),

  // private_table (explicit private)
  table(
    { name: 'private_table', public: false },
    {
      name: t.string(),
    }
  ),

  // points (private) with multi-column btree index (x, y)
  table(
    {
      name: 'points',
      public: false,
      indexes: [
        { name: 'multi_column_index', algorithm: 'btree', columns: ['x', 'y'] },
      ],
    },
    {
      x: t.i64(),
      y: t.i64(),
    }
  ),

  // pk_multi_identity with multiple constraints
  table(
    { name: 'pk_multi_identity' },
    {
      id: t.u32().primaryKey(),
      other: t.u32().unique().autoInc(),
    }
  ),

  // repeating_test_arg table with scheduled(repeating_test)
  table(
    {
      name: 'repeating_test_arg',
      // If your TS bindings support scheduling metadata, keep this line;
      // otherwise the reducer itself will reschedule.
      scheduled: 'repeating_test',
    } as any,
    {
      scheduled_id: t.u64().primaryKey().autoInc(),
      // Replace with your actual ScheduleAt type if named differently.
      scheduled_at: t.scheduleAt(),
      prev_time: t.timestamp(),
    }
  ),

  // has_special_stuff with Identity and ConnectionId
  table(
    { name: 'has_special_stuff' },
    {
      identity: t.identity(),
      connection_id: t.connectionId(),
    }
  ),

  // Two tables with the same row type: player and logged_out_player
  table(
    { name: 'player', public: true },
    {
      identity: t.identity().primaryKey(),
      player_id: t.u64().autoInc().unique(),
      name: t.string().unique(),
    }
  ),
  table(
    { name: 'logged_out_player', public: true },
    {
      identity: t.identity().primaryKey(),
      player_id: t.u64().autoInc().unique(),
      name: t.string().unique(),
    }
  )
);

// ─────────────────────────────────────────────────────────────────────────────
// REDUCERS (behavioral parity with Rust)
// ─────────────────────────────────────────────────────────────────────────────

// init: seed the repeating_test_arg table with a 1000ms recurring schedule
spacetimedb.reducer('init', {}, (ctx) => {
  ctx.db.repeating_test_arg.insert({
    prev_time: ctx.timestamp,
    scheduled_id: 0n, // u64 autoInc placeholder (engine will assign)
    scheduled_at: ScheduleAt.interval(1000000n), // 1000ms
  });
});

// repeating_test: log delta time since last run
spacetimedb.reducer('repeating_test', { arg: spacetimedb.tables.repeating_test_arg.row }, (ctx, { arg }) => {
  const delta = ctx.timestamp.since(arg.prev_time); // adjust if API differs
  console.trace(`Timestamp: ${ctx.timestamp}, Delta time: ${delta}`);
});

// add(name, age)
spacetimedb.reducer('add', { name: t.string(), age: t.u8() }, (ctx, { name, age }) => {
  ctx.db.person.insert({ id: 0, name, age });
});

// say_hello()
spacetimedb.reducer('say_hello', {}, (ctx) => {
  for (const person of ctx.db.person.iter()) {
    console.info(`Hello, ${person.name}!`);
  }
  console.info('Hello, World!');
});

// list_over_age(age)
spacetimedb.reducer('list_over_age', { age: t.u8() }, (ctx, { age }) => {
  // If your bindings expose an index accessor with range filtering, prefer it.
  // Example (pseudo): for (const person of ctx.db.person.age.filter([age, undefined])) { ... }
  // Fallback: iterate and filter in memory.
  for (const person of ctx.db.person.iter()) {
    if (person.age >= age) {
      console.info(`${person.name} has age ${person.age} >= ${age}`);
    }
  }
});

// log_module_identity()
spacetimedb.reducer('log_module_identity', {}, (ctx) => {
  console.info(`Module identity: ${ctx.identity()}`);
});

// test(arg: TestAlias(TestA), arg2: TestB, arg3: TestC, arg4: TestF)
spacetimedb.reducer(
  'test',
  { arg: spacetimedb.tables.test_a.row, arg2: testB, arg3: testC, arg4: TestF },
  (ctx, { arg, arg2, arg3, arg4 }) => {
    console.info('BEGIN');
    console.info(`sender: ${ctx.sender}`);
    console.info(`timestamp: ${ctx.timestamp}`);
    console.info(`bar: ${arg2.foo}`);

    // TestC
    if (arg3.tag === 'Foo') console.info('Foo');
    else if (arg3.tag === 'Bar') console.info('Bar');

    // TestF
    if (arg4.tag === 'Foo') console.info('Foo');
    else if (arg4.tag === 'Bar') console.info('Bar');
    else if (arg4.tag === 'Baz') console.info(arg4.value);

    // Insert test_a rows
    for (let i = 0; i < 1000; i++) {
      ctx.db.test_a.insert({ x: (i >>> 0) + arg.x, y: (i >>> 0) + arg.y, z: 'Yo' });
    }

    const rowCountBefore = ctx.db.test_a.count();
    console.info(`Row count before delete: ${rowCountBefore}`);

    // Delete rows by the indexed column `x` in [5,10)
    // Prefer index deletion if exposed, fallback to filter+delete
    let numDeleted = 0;
    for (let x = 5; x < 10; x++) {
      // If your bindings provide: numDeleted += ctx.db.test_a.index('foo').delete(x)
      // we would use it; otherwise:
      for (const row of ctx.db.test_a.iter()) {
        if (row.x === x) {
          if (ctx.db.test_a.delete(row)) numDeleted++;
        }
      }
    }

    const rowCountAfter = ctx.db.test_a.count();
    if (rowCountBefore !== rowCountAfter + BigInt(numDeleted)) {
      console.error(
        `Started with ${rowCountBefore} rows, deleted ${numDeleted}, and wound up with ${rowCountAfter} rows... huh?`
      );
    }

    // try_insert TestE { id: 0, name: "Tyler" }
    try {
      const inserted = ctx.db.test_e.tryInsert({ id: 0n, name: 'Tyler' });
      console.info(`Inserted: ${JSON.stringify(inserted)}`);
    } catch (err) {
      console.info(`Error: ${String(err)}`);
    }

    console.info(`Row count after delete: ${rowCountAfter}`);

    const otherRowCount = ctx.db.test_a.count();
    console.info(`Row count filtered by condition: ${otherRowCount}`);

    console.info('MultiColumn');

    for (let i = 0; i < 1000; i++) {
      ctx.db.points.insert({ x: BigInt(i) + BigInt(arg.x), y: BigInt(i) + BigInt(arg.y) });
    }

    let multiRowCount = 0;
    for (const row of ctx.db.points.iter()) {
      if (row.x >= 0n && row.y <= 200n) multiRowCount++;
    }
    console.info(`Row count filtered by multi-column condition: ${multiRowCount}`);

    console.info('END');
  }
);

// add_player(name) -> Result<(), String>
spacetimedb.reducer('add_player', { name: t.string() }, (ctx, { name }) => {
  // Try insert-or-update by primary key index
  // If bindings expose id.tryInsertOrUpdate, use that; otherwise emulate.
  const rec = { id: 0n as bigint, name };
  const inserted = ctx.db.test_e.insert(rec); // id autoInc => always creates a new one
  // no-op: re-upsert the same row (mirrors Rust behavior)
  ctx.db.test_e.id.update(inserted);
});

// delete_player(id) -> Result<(), String>
spacetimedb.reducer('delete_player', { id: t.u64() }, (ctx, { id }) => {
  const ok = ctx.db.test_e.id.delete(id);
  if (!ok) throw new Error(`No TestE row with id ${id}`);
});

// delete_players_by_name(name) -> Result<(), String>
spacetimedb.reducer('delete_players_by_name', { name: t.string() }, (ctx, { name }) => {
  // Prefer indexed delete if available; fallback to filter + delete
  let deleted = 0;
  for (const row of ctx.db.test_e.iter()) {
    if (row.name === name) {
      if (ctx.db.test_e.delete(row)) deleted++;
    }
  }
  if (deleted === 0) throw new Error(`No TestE row with name ${JSON.stringify(name)}`);
  console.info(`Deleted ${deleted} player(s) with name ${JSON.stringify(name)}`);
});

// client_connected hook
spacetimedb.reducer('client_connected', {}, (_ctx) => {
  // no-op
});

// add_private(name)
spacetimedb.reducer('add_private', { name: t.string() }, (ctx, { name }) => {
  ctx.db.private_table.insert({ name });
});

// query_private()
spacetimedb.reducer('query_private', {}, (ctx) => {
  for (const row of ctx.db.private_table.iter()) {
    console.info(`Private, ${row.name}!`);
  }
  console.info('Private, World!');
});

// test_btree_index_args(): In Rust this function exists to type-check index
// signatures. We’ll provide a minimal TS analog that exercises the name index and
// multi-column index in basic, non-exhaustive ways (runtime correctness only).
spacetimedb.reducer('test_btree_index_args', {}, (ctx) => {
  const s = 'String';
  // Read through name index (preferred) or fallback to filter
  for (const row of ctx.db.test_e.iter()) {
    if (row.name === s || row.name === 'str') {
      // no-op
    }
  }
  // Multi-column (x,y) index: demo simple scans
  for (const row of ctx.db.points.iter()) {
    // demo access patterns
    void row;
  }
});

// assert_caller_identity_is_module_identity()
spacetimedb.reducer('assert_caller_identity_is_module_identity', {}, (ctx) => {
  const caller = ctx.sender;
  const owner = ctx.identity();
  if (String(caller) !== String(owner)) {
    throw new Error(`Caller ${caller} is not the owner ${owner}`);
  } else {
    console.info(`Called by the owner ${owner}`);
  }
});
