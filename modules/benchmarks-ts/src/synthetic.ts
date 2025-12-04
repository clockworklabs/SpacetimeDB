// STDB module used for benchmarks.
//
// This file is tightly bound to the `benchmarks` crate (`crates/bench`).
//
// The various tables in this file need to remain synced with `crates/bench/src/schemas.rs`
// Field orders, names, and types should be the same.
//
// We instantiate multiple copies of each table. These should be identical
// aside from indexing strategy. Table names must match the template:
//
// `{IndexStrategy}{TableName}`, in PascalCase.
//
// The reducers need to remain synced with `crates/bench/src/spacetime_module.rs`
// Reducer names must match the template:
//
// `{operation}_{index_strategy}_{table_name}`, in snake_case.
//
// The three index strategies are:
// - `unique`: a single unique key, declared first in the struct.
// - `no_index`: no indexes.
// - `btree_each_column`: one index for each column.
//
// Obviously more could be added...

import { blackBox } from './load';
import {
  spacetimedb,
  unique_0_u32_u64_u64_tRow,
  no_index_u32_u64_u64_tRow,
  btree_each_column_u32_u64_u64_tRow,
  unique_0_u32_u64_str_tRow,
  no_index_u32_u64_str_tRow,
  btree_each_column_u32_u64_str_tRow
} from './schema';
import {
  t,
} from 'spacetimedb/server';

// ---------- empty ----------

spacetimedb.reducer('empty', () => {});

// ---------- insert ----------

spacetimedb.reducer(
  'insert_unique_0_u32_u64_str',
  { id: t.u32(), age: t.u64(), name: t.string() },
  (ctx, { id, age, name }) => {
    ctx.db.unique0U32U64Str.insert({ id, name, age });
  });

spacetimedb.reducer(
  'insert_no_index_u32_u64_str',
  { id: t.u32(), age: t.u64(), name: t.string() },
  (ctx, { id, age, name }) => {
    ctx.db.noIndexU32U64Str.insert({ id, name, age });
  });

spacetimedb.reducer(
  'insert_btree_each_column_u32_u64_str',
  { id: t.u32(), age: t.u64(), name: t.string() },
  (ctx, { id, age, name }) => {
    ctx.db.btreeEachColumnU32U64Str.insert({ id, name, age });
  });

spacetimedb.reducer(
  'insert_unique_0_u32_u64_u64',
  { id: t.u32(), x: t.u64(), y: t.u64() },
  (ctx, { id, x, y }) => {
    ctx.db.unique0U32U64U64.insert({ id, x, y });
  });

spacetimedb.reducer(
  'insert_no_index_u32_u64_u64',
  { id: t.u32(), x: t.u64(), y: t.u64() },
  (ctx, { id, x, y }) => {
    ctx.db.noIndexU32U64U64.insert({ id, x, y });
  });

spacetimedb.reducer(
  'insert_btree_each_column_u32_u64_u64',
  { id: t.u32(), x: t.u64(), y: t.u64() },
  (ctx, { id, x, y }) => {
    ctx.db.btreeEachColumnU32U64U64.insert({ id, x, y });
  });

// ---------- insert bulk ----------

spacetimedb.reducer(
  'insert_bulk_unique_0_u32_u64_u64',
  { locs: t.array(unique_0_u32_u64_u64_tRow) },
  (ctx, { locs }) => {
    for (const loc of locs) {
      ctx.db.unique0U32U64U64.insert(loc);
    }
  });

spacetimedb.reducer(
  'insert_bulk_no_index_u32_u64_u64',
  { locs: t.array(no_index_u32_u64_u64_tRow) },
  (ctx, { locs }) => {
    for (const loc of locs) {
      ctx.db.noIndexU32U64U64.insert(loc);
    }
  });

spacetimedb.reducer(
  'insert_bulk_btree_each_column_u32_u64_u64',
  { locs: t.array(btree_each_column_u32_u64_u64_tRow) },
  (ctx, { locs }) => {
    for (const loc of locs) {
      ctx.db.btreeEachColumnU32U64U64.insert(loc);
    }
  });

spacetimedb.reducer(
  'insert_bulk_unique_0_u32_u64_str',
  { people: t.array(unique_0_u32_u64_str_tRow) },
  (ctx, { people }) => {
    for (const p of people) {
      ctx.db.unique0U32U64Str.insert(p);
    }
  });

spacetimedb.reducer(
  'insert_bulk_no_index_u32_u64_str',
  { people: t.array(no_index_u32_u64_str_tRow) },
  (ctx, { people }) => {
    for (const p of people) {
      ctx.db.noIndexU32U64Str.insert(p);
    }
  });

spacetimedb.reducer(
  'insert_bulk_btree_each_column_u32_u64_str',
  { people: t.array(btree_each_column_u32_u64_str_tRow) },
  (ctx, { people }) => {
    for (const p of people) {
      ctx.db.btreeEachColumnU32U64Str.insert(p);
    }
  });

// ---------- update ----------

function assert(cond: boolean) {
  if (!cond) {
    throw new Error("assertion failed");
  }
}

spacetimedb.reducer(
  'update_bulk_unique_0_u32_u64_u64',
  { row_count: t.u32() },
  (ctx, { row_count }) => {
    let hit = 0;
    for (const loc of ctx.db.unique0U32U64U64.iter()) {
      if (hit == row_count) {
        break;
      }

      hit += 1;
      ctx.db.unique0U32U64U64.id?.update({
        id: loc.id,
        x: loc.x + 1n,
        y: loc.y,
      });
    }

    assert(hit == row_count);
  });

spacetimedb.reducer(
  'update_bulk_unique_0_u32_u64_str',
  { row_count: t.u32() },
  (ctx, { row_count }) => {
    let hit = 0;
    for (const p of ctx.db.unique0U32U64Str.iter()) {
      if (hit == row_count) {
        break;
      }

      hit += 1;
      ctx.db.unique0U32U64Str.id?.update({
        id: p.id,
        age: p.age + 1n,
        name: p.name,
      });
    }

    assert(hit == row_count);
  });

// ---------- iterate ----------

spacetimedb.reducer('iterate_unique_0_u32_u64_str', ctx => {
  for (const x of ctx.db.unique0U32U64Str.iter()) {
    blackBox(x);
  }
});

spacetimedb.reducer('iterate_unique_0_u32_u64_u64', ctx => {
  for (const x of ctx.db.unique0U32U64U64.iter()) {
    blackBox(x);
  }
});

// ---------- filtering ----------

spacetimedb.reducer('filter_unique_0_u32_u64_str_by_id', { id: t.u32() }, (ctx, { id }) => {
  blackBox(ctx.db.unique0U32U64Str.id?.find(id));
});

spacetimedb.reducer('filter_no_index_u32_u64_str_by_id', { id: t.u32() }, (ctx, { id }) => {
  for (const r of ctx.db.noIndexU32U64Str.iter()) {
    if (r.id == id) {
      blackBox(r);
    }
  }
});

spacetimedb.reducer('filter_btree_each_column_u32_u64_str_by_id', { id: t.u32() }, (ctx, { id }) => {
  const idIndex = ctx.db.btreeEachColumnU32U64Str.id;
  if (idIndex) {
    for (const r of idIndex.filter(id)) {
      blackBox(r);
    }
  }
});

spacetimedb.reducer('filter_unique_0_u32_u64_str_by_name', { name: t.string() }, (ctx, { name }) => {
  for (const r of ctx.db.unique0U32U64Str.iter()) {
    if (r.name == name) {
      blackBox(r);
    }
  }
});

spacetimedb.reducer('filter_no_index_u32_u64_str_by_name', { name: t.string() }, (ctx, { name }) => {
  for (const r of ctx.db.noIndexU32U64Str.iter()) {
    if (r.name == name) {
      blackBox(r);
    }
  }
});

spacetimedb.reducer('filter_btree_each_column_u32_u64_str_by_name', { name: t.string() }, (ctx, { name }) => {
  const nameIndex = ctx.db.btreeEachColumnU32U64Str.name;
  if (nameIndex) {
    for (const r of nameIndex.filter(name)) {
      blackBox(r);
    }
  }
});

spacetimedb.reducer('filter_unique_0_u32_u64_u64_by_id', { id: t.u32() }, (ctx, { id }) => {
  blackBox(ctx.db.unique0U32U64U64.id?.find(id));
});

spacetimedb.reducer('filter_no_index_u32_u64_u64_by_id', { id: t.u32() }, (ctx, { id }) => {
  for (const r of ctx.db.noIndexU32U64U64.iter()) {
    if (r.id == id) {
      blackBox(r);
    }
  }
});

spacetimedb.reducer('filter_btree_each_column_u32_u64_u64_by_id', { id: t.u32() }, (ctx, { id }) => {
  const idIndex = ctx.db.btreeEachColumnU32U64U64.id;
  if (idIndex) {
    for (const r of idIndex.filter(id)) {
      blackBox(r);
    }
  }
});

spacetimedb.reducer('filter_unique_0_u32_u64_u64_by_x', { x: t.u64() }, (ctx, { x }) => {
  for (const r of ctx.db.unique0U32U64U64.iter()) {
    if (r.x == x) {
      blackBox(r);
    }
  }
});

spacetimedb.reducer('filter_no_index_u32_u64_u64_by_x', { x: t.u64() }, (ctx, { x }) => {
  for (const r of ctx.db.noIndexU32U64U64.iter()) {
    if (r.x == x) {
      blackBox(r);
    }
  }
});

spacetimedb.reducer('filter_btree_each_column_u32_u64_u64_by_x', { x: t.u64() }, (ctx, { x }) => {
  const xIndex = ctx.db.btreeEachColumnU32U64U64.x;
  if (xIndex) {
    for (const r of xIndex.filter(x)) {
      blackBox(r);
    }
  }
});

spacetimedb.reducer('filter_unique_0_u32_u64_u64_by_y', { y: t.u64() }, (ctx, { y }) => {
  for (const r of ctx.db.unique0U32U64U64.iter()) {
    if (r.y == y) {
      blackBox(r);
    }
  }
});

spacetimedb.reducer('filter_no_index_u32_u64_u64_by_y', { y: t.u64() }, (ctx, { y }) => {
  for (const r of ctx.db.noIndexU32U64U64.iter()) {
    if (r.y == y) {
      blackBox(r);
    }
  }
});

spacetimedb.reducer('filter_btree_each_column_u32_u64_u64_by_y', { y: t.u64() }, (ctx, { y }) => {
  const yIndex = ctx.db.btreeEachColumnU32U64U64.y;
  if (yIndex) {
    for (const r of yIndex.filter(y)) {
      blackBox(r);
    }
  }
});


// ---------- delete ----------

// FIXME: current nonunique delete interface is UNUSABLE!!!!

spacetimedb.reducer('delete_unique_0_u32_u64_str_by_id', { id: t.u32() }, (ctx, { id }) => {
  ctx.db.unique0U32U64Str.id?.delete(id);
});

spacetimedb.reducer('delete_unique_0_u32_u64_u64_by_id', { id: t.u32() }, (ctx, { id }) => {
  ctx.db.unique0U32U64U64.id?.delete(id);
});

// ---------- clear table ----------

function unimplemented() {
  throw new Error('Modules currently have no interface to clear a table');
}

spacetimedb.reducer('clear_table_unique_0_u32_u64_str', () => {
  unimplemented();
});

spacetimedb.reducer('clear_table_no_index_u32_u64_str', () => {
  unimplemented();
});

spacetimedb.reducer('clear_table_btree_each_column_u32_u64_str', () => {
  unimplemented();
});

spacetimedb.reducer('clear_table_unique_0_u32_u64_u64', () => {
  unimplemented();
});

spacetimedb.reducer('clear_table_no_index_u32_u64_u64', () => {
  unimplemented();
});

spacetimedb.reducer('clear_table_btree_each_column_u32_u64_u64', () => {
  unimplemented();
});

// ---------- count ----------

// You need to inspect the module outputs to actually read the result from these.

spacetimedb.reducer('count_unique_0_u32_u64_str', ctx => {
  const count = ctx.db.unique0U32U64Str.count();
  console.info!(`COUNT: ${count}`);
});

spacetimedb.reducer('count_no_index_u32_u64_str', ctx => {
  const count = ctx.db.noIndexU32U64Str.count();
  console.info!(`COUNT: ${count}`);
});

spacetimedb.reducer('count_btree_each_column_u32_u64_str', ctx => {
  const count = ctx.db.btreeEachColumnU32U64Str.count();
  console.info!(`COUNT: ${count}`);
});

spacetimedb.reducer('count_unique_0_u32_u64_u64', ctx => {
  const count = ctx.db.unique0U32U64U64.count();
  console.info!(`COUNT: ${count}`);
});

spacetimedb.reducer('count_no_index_u32_u64_u64', ctx => {
  const count = ctx.db.noIndexU32U64U64.count();
  console.info!(`COUNT: ${count}`);
});

spacetimedb.reducer('count_btree_each_column_u32_u64_u64', ctx => {
  const count = ctx.db.btreeEachColumnU32U64U64.count();
  console.info!(`COUNT: ${count}`);
});

// ---------- module-specific stuff ----------

spacetimedb.reducer('fn_with_1_args', { _arg: t.string() }, (ctx, { _arg }) => {
  blackBox(_arg);
});

spacetimedb.reducer(
  'fn_with_32_args',
  {
    _arg1: t.string(),
    _arg2: t.string(),
    _arg3: t.string(),
    _arg4: t.string(),
    _arg5: t.string(),
    _arg6: t.string(),
    _arg7: t.string(),
    _arg8: t.string(),
    _arg9: t.string(),
    _arg10: t.string(),
    _arg11: t.string(),
    _arg12: t.string(),
    _arg13: t.string(),
    _arg14: t.string(),
    _arg15: t.string(),
    _arg16: t.string(),
    _arg17: t.string(),
    _arg18: t.string(),
    _arg19: t.string(),
    _arg20: t.string(),
    _arg21: t.string(),
    _arg22: t.string(),
    _arg23: t.string(),
    _arg24: t.string(),
    _arg25: t.string(),
    _arg26: t.string(),
    _arg27: t.string(),
    _arg28: t.string(),
    _arg29: t.string(),
    _arg30: t.string(),
    _arg31: t.string(),
    _arg32: t.string(),
  },
  // eslint-disable-next-line @typescript-eslint/no-unused-vars
  (ctx, {
    _arg1,
    _arg2,
    _arg3,
    _arg4,
    _arg5,
    _arg6,
    _arg7,
    _arg8,
    _arg9,
    _arg10,
    _arg11,
    _arg12,
    _arg13,
    _arg14,
    _arg15,
    _arg16,
    _arg17,
    _arg18,
    _arg19,
    _arg20,
    _arg21,
    _arg22,
    _arg23,
    _arg24,
    _arg25,
    _arg26,
    _arg27,
    _arg28,
    _arg29,
    _arg30,
    _arg31,
    _arg32,
  }) => {
  });

spacetimedb.reducer('print_many_things', { n: t.u32() }, (ctx, { n }) => {
  for (let i = 0; i < n; i++) {
    console.log("hello again!");
  }
});
