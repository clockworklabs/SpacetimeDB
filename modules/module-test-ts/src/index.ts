// ─────────────────────────────────────────────────────────────────────────────
// IMPORTS
// ─────────────────────────────────────────────────────────────────────────────
import { ScheduleAt } from 'spacetimedb';
import {
  schema,
  table,
  t,
  type Infer,
  type InferTypeOfRow,
} from 'spacetimedb/server';

// ─────────────────────────────────────────────────────────────────────────────
// TYPE ALIASES
// ─────────────────────────────────────────────────────────────────────────────
// Rust: pub type TestAlias = TestA
type TestAlias = TestA;

// ─────────────────────────────────────────────────────────────────────────────
// SUPPORT TYPES (SpacetimeType equivalents)
// ─────────────────────────────────────────────────────────────────────────────

// Rust: #[derive(SpacetimeType)] pub struct TestB { foo: String }
const testB = t.object('TestB', {
  foo: t.string(),
});
type TestB = Infer<typeof testB>;

// Rust: #[derive(SpacetimeType)] #[sats(name = "Namespace.TestC")] enum TestC { Foo, Bar }
const testC = t.enum('Namespace.TestC', {
  Foo: t.unit(),
  Bar: t.unit(),
});
type TestC = Infer<typeof testC>;

// Rust: const DEFAULT_TEST_C: TestC = TestC::Foo;
const DEFAULT_TEST_C: TestC = { tag: 'Foo', value: {} } as const;

// Rust: #[derive(SpacetimeType)] pub struct Baz { pub field: String }
const Baz = t.object('Baz', {
  field: t.string(),
});
type Baz = Infer<typeof Baz>;

// Rust: #[derive(SpacetimeType)] pub enum Foobar { Baz(Baz), Bar, Har(u32) }
const foobar = t.enum('Foobar', {
  Baz: Baz,
  Bar: t.unit(),
  Har: t.u32(),
});
type Foobar = Infer<typeof foobar>;

// Rust: #[derive(SpacetimeType)] #[sats(name = "Namespace.TestF")] enum TestF { Foo, Bar, Baz(String) }
const testF = t.enum('Namespace.TestF', {
  Foo: t.unit(),
  Bar: t.unit(),
  Baz: t.string(),
});
type TestF = Infer<typeof testF>;

// Rust: #[derive(Deserialize)] pub struct Foo<'a> { pub field: &'a str }
// In TS we simply model as a struct and provide a BSATN deserializer placeholder.
const Foo = t.object('Foo', { field: t.string() });
type Foo = Infer<typeof Foo>;
function Foo_baz_bsatn(_bytes: Uint8Array): Foo {
  // If your bindings expose a bsatn decode helper, use it here.
  // return bsatn.fromSlice(bytes, Foo);
  throw new Error('Implement BSATN decode for Foo if needed');
}

// ─────────────────────────────────────────────────────────────────────────────
// TABLE ROW DEFINITIONS (shape only)
// ─────────────────────────────────────────────────────────────────────────────

// Rust: #[spacetimedb::table(name = person, public, index(name = age, btree(columns = [age])))]
const personRow = {
  id: t.u32().primaryKey().autoInc(),
  name: t.string(),
  age: t.u8(),
};

// Rust: #[spacetimedb::table(name = test_a, index(name = foo, btree(columns = [x])))]
const testA = t.row({
  x: t.u32(),
  y: t.u32(),
  z: t.string(),
});
type TestA = Infer<typeof testA>;

// Rust: #[table(name = test_d, public)] struct TestD { #[default(Some(DEFAULT_TEST_C))] test_c: Option<TestC>, }
// NOTE: If your Option default requires wrapping, adjust to your bindings’ Option encoding.
const testDRow = {
  test_c: t.option(testC).default(DEFAULT_TEST_C as unknown as any),
};
type TestD = InferTypeOfRow<typeof testDRow>;

// Rust: #[spacetimedb::table(name = test_e)] #[derive(Debug)]
const testERow = {
  id: t.u64().primaryKey().autoInc(),
  name: t.string(),
};

// Rust: #[table(name = test_f, public)] pub struct TestFoobar { pub field: Foobar }
const testFRow = {
  field: foobar,
};

// Rust: #[spacetimedb::table(name = private_table, private)]
const privateTableRow = {
  name: t.string(),
};

// Rust: #[spacetimedb::table(name = points, private, index(name = multi_column_index, btree(columns = [x, y])))]
const pointsRow = {
  x: t.i64(),
  y: t.i64(),
};

// Rust: #[spacetimedb::table(name = pk_multi_identity)]
const pkMultiIdentityRow = {
  id: t.u32().primaryKey(),
  other: t.u32().unique().autoInc(),
};

// Rust: #[spacetimedb::table(name = repeating_test_arg, scheduled(repeating_test))]
const repeatingTestArg = t.row({
  scheduled_id: t.u64().primaryKey().autoInc(),
  scheduled_at: t.scheduleAt(),
  prev_time: t.timestamp(),
});
type RepeatingTestArg = Infer<typeof repeatingTestArg>;

// Rust: #[spacetimedb::table(name = has_special_stuff)]
const hasSpecialStuffRow = {
  identity: t.identity(),
  connection_id: t.connectionId(),
};

// Rust: two tables with the same row type: player & logged_out_player
const playerLikeRow = {
  identity: t.identity().primaryKey(),
  player_id: t.u64().autoInc().unique(),
  name: t.string().unique(),
};

// ─────────────────────────────────────────────────────────────────────────────
// SCHEMA (tables + indexes + visibility)
// ─────────────────────────────────────────────────────────────────────────────
const spacetimedb = schema(
  // person (public) with btree index on age
  table(
    {
      name: 'person',
      public: true,
      indexes: [{ name: 'age', algorithm: 'btree', columns: ['age'] }],
    },
    personRow
  ),

  // test_a with index foo on x
  table(
    {
      name: 'test_a',
      indexes: [{ name: 'foo', algorithm: 'btree', columns: ['x'] }],
    },
    testA
  ),

  // test_d (public) with default(Some(DEFAULT_TEST_C)) option field
  table({ name: 'test_d', public: true }, testDRow),

  // test_e, default private, with primary key id auto_inc and btree index on name
  table(
    {
      name: 'test_e',
      public: false,
      indexes: [{ name: 'name', algorithm: 'btree', columns: ['name'] }],
    },
    testERow
  ),

  // test_f (public) with Foobar field
  table({ name: 'test_f', public: true }, testFRow),

  // private_table (explicit private)
  table({ name: 'private_table', public: false }, privateTableRow),

  // points (private) with multi-column btree index (x, y)
  table(
    {
      name: 'points',
      public: false,
      indexes: [
        { name: 'multi_column_index', algorithm: 'btree', columns: ['x', 'y'] },
      ],
    },
    pointsRow
  ),

  // pk_multi_identity with multiple constraints
  table({ name: 'pk_multi_identity' }, pkMultiIdentityRow),

  // repeating_test_arg table with scheduled(repeating_test)
  table(
    { name: 'repeating_test_arg', scheduled: 'repeating_test' } as any,
    repeatingTestArg
  ),

  // has_special_stuff with Identity and ConnectionId
  table({ name: 'has_special_stuff' }, hasSpecialStuffRow),

  // Two tables with the same row type: player and logged_out_player
  table({ name: 'player', public: true }, playerLikeRow),
  table({ name: 'logged_out_player', public: true }, playerLikeRow)
);

// ─────────────────────────────────────────────────────────────────────────────
// REDUCERS (mirroring Rust order & behavior)
// ─────────────────────────────────────────────────────────────────────────────

// init
spacetimedb.reducer('init', {}, ctx => {
  ctx.db.repeating_test_arg.insert({
    prev_time: ctx.timestamp,
    scheduled_id: 0n, // u64 autoInc placeholder (engine will assign)
    scheduled_at: ScheduleAt.interval(1000000n), // 1000ms
  });
});

// repeating_test
spacetimedb.reducer(
  'repeating_test',
  { arg: repeatingTestArg },
  (ctx, { arg }) => {
    const delta = ctx.timestamp.since(arg.prev_time); // adjust if API differs
    console.trace(`Timestamp: ${ctx.timestamp}, Delta time: ${delta}`);
  }
);

// add(name, age)
spacetimedb.reducer(
  'add',
  { name: t.string(), age: t.u8() },
  (ctx, { name, age }) => {
    ctx.db.person.insert({ id: 0, name, age });
  }
);

// say_hello()
spacetimedb.reducer('say_hello', {}, ctx => {
  for (const person of ctx.db.person.iter()) {
    console.info(`Hello, ${person.name}!`);
  }
  console.info('Hello, World!');
});

// list_over_age(age)
spacetimedb.reducer('list_over_age', { age: t.u8() }, (ctx, { age }) => {
  // Prefer an index-based scan if exposed by bindings; otherwise iterate.
  for (const person of ctx.db.person.iter()) {
    if (person.age >= age) {
      console.info(`${person.name} has age ${person.age} >= ${age}`);
    }
  }
});

// log_module_identity()
spacetimedb.reducer('log_module_identity', {}, ctx => {
  console.info(`Module identity: ${ctx.identity}`);
});

// test(arg: TestAlias(TestA), arg2: TestB, arg3: TestC, arg4: TestF)
spacetimedb.reducer(
  'test',
  { arg: testA, arg2: testB, arg3: testC, arg4: testF },
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
      ctx.db.test_a.insert({
        x: (i >>> 0) + arg.x,
        y: (i >>> 0) + arg.y,
        z: 'Yo',
      });
    }

    const rowCountBefore = ctx.db.test_a.count();
    console.info(`Row count before delete: ${rowCountBefore}`);

    // Delete rows by the indexed column `x` in [5,10)
    let numDeleted = 0;
    for (let x = 5; x < 10; x++) {
      // Prefer index deletion if available; fallback to filter+delete
      for (const row of ctx.db.test_a.iter()) {
        if (row.x === x) {
          if (ctx.db.test_a.delete(row)) numDeleted++;
        }
      }
    }

    const rowCountAfter = ctx.db.test_a.count();
    if (Number(rowCountBefore) !== Number(rowCountAfter) + numDeleted) {
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
      ctx.db.points.insert({
        x: BigInt(i) + BigInt(arg.x),
        y: BigInt(i) + BigInt(arg.y),
      });
    }

    let multiRowCount = 0;
    for (const row of ctx.db.points.iter()) {
      if (row.x >= 0n && row.y <= 200n) multiRowCount++;
    }
    console.info(
      `Row count filtered by multi-column condition: ${multiRowCount}`
    );

    console.info('END');
  }
);

// add_player(name) -> Result<(), String>
spacetimedb.reducer('add_player', { name: t.string() }, (ctx, { name }) => {
  const rec = { id: 0n as bigint, name };
  const inserted = ctx.db.test_e.insert(rec); // id autoInc => always creates a new one
  // No-op re-upsert by id index if your bindings support it.
  if (ctx.db.test_e.id?.update) ctx.db.test_e.id.update(inserted);
});

// delete_player(id) -> Result<(), String>
spacetimedb.reducer('delete_player', { id: t.u64() }, (ctx, { id }) => {
  const ok = ctx.db.test_e.id.delete(id);
  if (!ok) throw new Error(`No TestE row with id ${id}`);
});

// delete_players_by_name(name) -> Result<(), String>
spacetimedb.reducer(
  'delete_players_by_name',
  { name: t.string() },
  (ctx, { name }) => {
    let deleted = 0;
    for (const row of ctx.db.test_e.iter()) {
      if (row.name === name) {
        if (ctx.db.test_e.delete(row)) deleted++;
      }
    }
    if (deleted === 0)
      throw new Error(`No TestE row with name ${JSON.stringify(name)}`);
    console.info(
      `Deleted ${deleted} player(s) with name ${JSON.stringify(name)}`
    );
  }
);

// client_connected hook
spacetimedb.reducer('client_connected', {}, _ctx => {
  // no-op
});

// add_private(name)
spacetimedb.reducer('add_private', { name: t.string() }, (ctx, { name }) => {
  ctx.db.private_table.insert({ name });
});

// query_private()
spacetimedb.reducer('query_private', {}, ctx => {
  for (const row of ctx.db.private_table.iter()) {
    console.info(`Private, ${row.name}!`);
  }
  console.info('Private, World!');
});

// test_btree_index_args
// (In Rust this exists to type-check various index argument forms.)
spacetimedb.reducer('test_btree_index_args', {}, ctx => {
  const s = 'String';
  // Demonstrate scanning via iteration; prefer index access if bindings expose it.
  for (const row of ctx.db.test_e.iter()) {
    if (row.name === s || row.name === 'str') {
      // no-op; exercising types
    }
  }
  for (const row of ctx.db.points.iter()) {
    void row; // exercise multi-column index presence
  }
});

// assert_caller_identity_is_module_identity
spacetimedb.reducer('assert_caller_identity_is_module_identity', {}, ctx => {
  const caller = ctx.sender;
  const owner = ctx.identity;
  if (String(caller) !== String(owner)) {
    throw new Error(`Caller ${caller} is not the owner ${owner}`);
  } else {
    console.info(`Called by the owner ${owner}`);
  }
});
