using SpacetimeDB;

namespace Benchmarks;

public static partial class synthetic
{
    // ---------- schemas ----------

    [SpacetimeDB.Table(Name = "unique_0_u32_u64_str")]
    public partial struct unique_0_u32_u64_str_t
    {
        [Unique]
        public uint id;
        public ulong age;
        public string name;
    }

    [SpacetimeDB.Table(Name = "no_index_u32_u64_str")]
    public partial struct no_index_u32_u64_str_t
    {
        public uint id;
        public ulong age;
        public string name;
    }

    [SpacetimeDB.Table(Name = "btree_each_column_u32_u64_str")]
    [SpacetimeDB.Index(BTree = [nameof(id)])]
    [SpacetimeDB.Index(BTree = [nameof(age)])]
    [SpacetimeDB.Index(BTree = [nameof(name)])]
    public partial struct btree_each_column_u32_u64_str_t
    {
        public uint id;
        public ulong age;
        public string name;
    }

    [SpacetimeDB.Table(Name = "unique_0_u32_u64_u64")]
    public partial struct unique_0_u32_u64_u64_t
    {
        [Unique]
        public uint id;
        public ulong x;
        public ulong y;
    }

    [SpacetimeDB.Table(Name = "no_index_u32_u64_u64")]
    public partial struct no_index_u32_u64_u64_t
    {
        public uint id;
        public ulong x;
        public ulong y;
    }

    [SpacetimeDB.Table(Name = "btree_each_column_u32_u64_u64")]
    [SpacetimeDB.Index(BTree = [nameof(id)])]
    [SpacetimeDB.Index(BTree = [nameof(x)])]
    [SpacetimeDB.Index(BTree = [nameof(y)])]
    public partial struct btree_each_column_u32_u64_u64_t
    {
        public uint id;
        public ulong x;
        public ulong y;
    }

    // ---------- empty ----------

    [SpacetimeDB.Reducer]
    public static void empty(ReducerContext ctx) { }

    // ---------- insert ----------

    [SpacetimeDB.Reducer]
    public static void insert_unique_0_u32_u64_str(
        ReducerContext ctx,
        uint id,
        ulong age,
        string name
    )
    {
        ctx.Db.unique_0_u32_u64_str.Insert(
            new()
            {
                id = id,
                age = age,
                name = name,
            }
        );
    }

    [SpacetimeDB.Reducer]
    public static void insert_no_index_u32_u64_str(
        ReducerContext ctx,
        uint id,
        ulong age,
        string name
    )
    {
        ctx.Db.no_index_u32_u64_str.Insert(
            new()
            {
                id = id,
                age = age,
                name = name,
            }
        );
    }

    [SpacetimeDB.Reducer]
    public static void insert_btree_each_column_u32_u64_str(
        ReducerContext ctx,
        uint id,
        ulong age,
        string name
    )
    {
        ctx.Db.btree_each_column_u32_u64_str.Insert(
            new()
            {
                id = id,
                age = age,
                name = name,
            }
        );
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_0_u32_u64_u64(ReducerContext ctx, uint id, ulong x, ulong y)
    {
        ctx.Db.unique_0_u32_u64_u64.Insert(
            new()
            {
                id = id,
                x = x,
                y = y,
            }
        );
    }

    [SpacetimeDB.Reducer]
    public static void insert_no_index_u32_u64_u64(ReducerContext ctx, uint id, ulong x, ulong y)
    {
        ctx.Db.no_index_u32_u64_u64.Insert(
            new()
            {
                id = id,
                x = x,
                y = y,
            }
        );
    }

    [SpacetimeDB.Reducer]
    public static void insert_btree_each_column_u32_u64_u64(
        ReducerContext ctx,
        uint id,
        ulong x,
        ulong y
    )
    {
        ctx.Db.btree_each_column_u32_u64_u64.Insert(
            new()
            {
                id = id,
                x = x,
                y = y,
            }
        );
    }

    // ---------- insert bulk ----------

    [SpacetimeDB.Reducer]
    public static void insert_bulk_unique_0_u32_u64_u64(
        ReducerContext ctx,
        List<unique_0_u32_u64_u64_t> locs
    )
    {
        foreach (unique_0_u32_u64_u64_t loc in locs)
        {
            ctx.Db.unique_0_u32_u64_u64.Insert(loc);
        }
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_no_index_u32_u64_u64(
        ReducerContext ctx,
        List<no_index_u32_u64_u64_t> locs
    )
    {
        foreach (no_index_u32_u64_u64_t loc in locs)
        {
            ctx.Db.no_index_u32_u64_u64.Insert(loc);
        }
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_btree_each_column_u32_u64_u64(
        ReducerContext ctx,
        List<btree_each_column_u32_u64_u64_t> locs
    )
    {
        foreach (btree_each_column_u32_u64_u64_t loc in locs)
        {
            ctx.Db.btree_each_column_u32_u64_u64.Insert(loc);
        }
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_unique_0_u32_u64_str(
        ReducerContext ctx,
        List<unique_0_u32_u64_str_t> people
    )
    {
        foreach (unique_0_u32_u64_str_t u32_u64_str in people)
        {
            ctx.Db.unique_0_u32_u64_str.Insert(u32_u64_str);
        }
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_no_index_u32_u64_str(
        ReducerContext ctx,
        List<no_index_u32_u64_str_t> people
    )
    {
        foreach (no_index_u32_u64_str_t u32_u64_str in people)
        {
            ctx.Db.no_index_u32_u64_str.Insert(u32_u64_str);
        }
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_btree_each_column_u32_u64_str(
        ReducerContext ctx,
        List<btree_each_column_u32_u64_str_t> people
    )
    {
        foreach (btree_each_column_u32_u64_str_t u32_u64_str in people)
        {
            ctx.Db.btree_each_column_u32_u64_str.Insert(u32_u64_str);
        }
    }

    // ---------- update ----------

    [SpacetimeDB.Reducer]
    public static void update_bulk_unique_0_u32_u64_u64(ReducerContext ctx, uint row_count)
    {
        int hit = 0;
        foreach (
            unique_0_u32_u64_u64_t loc in ctx.Db.unique_0_u32_u64_u64.Iter().Take((int)row_count)
        )
        {
            hit++;
            ctx.Db.unique_0_u32_u64_u64.id.Update(
                new()
                {
                    id = loc.id,
                    x = loc.x + 1,
                    y = loc.y,
                }
            );
        }
        if (hit != row_count)
        {
            throw new Exception("Not enough rows to perform requested amount of updates");
        }
    }

    [SpacetimeDB.Reducer]
    public static void update_bulk_unique_0_u32_u64_str(ReducerContext ctx, uint row_count)
    {
        uint hit = 0;
        foreach (
            unique_0_u32_u64_str_t u32_u64_str in ctx
                .Db.unique_0_u32_u64_str.Iter()
                .Take((int)row_count)
        )
        {
            hit++;
            ctx.Db.unique_0_u32_u64_str.id.Update(
                new()
                {
                    id = u32_u64_str.id,
                    name = u32_u64_str.name,
                    age = u32_u64_str.age + 1,
                }
            );
        }
        if (hit != row_count)
        {
            throw new Exception("Not enough rows to perform requested amount of updates");
        }
    }

    // ---------- iterate ----------

    [SpacetimeDB.Reducer]
    public static void iterate_unique_0_u32_u64_str(ReducerContext ctx)
    {
        foreach (unique_0_u32_u64_str_t u32_u64_str in ctx.Db.unique_0_u32_u64_str.Iter())
        {
            Bench.BlackBox(u32_u64_str);
        }
    }

    [SpacetimeDB.Reducer]
    public static void iterate_unique_0_u32_u64_u64(ReducerContext ctx)
    {
        foreach (unique_0_u32_u64_u64_t u32_u64_u64 in ctx.Db.unique_0_u32_u64_u64.Iter())
        {
            Bench.BlackBox(u32_u64_u64);
        }
    }

    // ---------- filtering ----------

    [SpacetimeDB.Reducer]
    public static void filter_unique_0_u32_u64_str_by_id(ReducerContext ctx, uint id)
    {
        Bench.BlackBox(ctx.Db.unique_0_u32_u64_str.id.Find(id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_no_index_u32_u64_str_by_id(ReducerContext ctx, uint id)
    {
        Bench.BlackBox(ctx.Db.no_index_u32_u64_str.Iter().Where(row => row.id == id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_btree_each_column_u32_u64_str_by_id(ReducerContext ctx, uint id)
    {
        Bench.BlackBox(ctx.Db.btree_each_column_u32_u64_str.id.Filter(id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_unique_0_u32_u64_str_by_name(ReducerContext ctx, string name)
    {
        Bench.BlackBox(ctx.Db.unique_0_u32_u64_str.Iter().Where(row => row.name == name));
    }

    [SpacetimeDB.Reducer]
    public static void filter_no_index_u32_u64_str_by_name(ReducerContext ctx, string name)
    {
        Bench.BlackBox(ctx.Db.no_index_u32_u64_str.Iter().Where(row => row.name == name));
    }

    [SpacetimeDB.Reducer]
    public static void filter_btree_each_column_u32_u64_str_by_name(ReducerContext ctx, string name)
    {
        Bench.BlackBox(ctx.Db.btree_each_column_u32_u64_str.name.Filter(name));
    }

    [SpacetimeDB.Reducer]
    public static void filter_unique_0_u32_u64_u64_by_id(ReducerContext ctx, uint id)
    {
        Bench.BlackBox(ctx.Db.unique_0_u32_u64_u64.id.Find(id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_no_index_u32_u64_u64_by_id(ReducerContext ctx, uint id)
    {
        Bench.BlackBox(ctx.Db.no_index_u32_u64_u64.Iter().Where(row => row.id == id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_btree_each_column_u32_u64_u64_by_id(ReducerContext ctx, uint id)
    {
        Bench.BlackBox(ctx.Db.btree_each_column_u32_u64_u64.Iter().Where(row => row.id == id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_unique_0_u32_u64_u64_by_x(ReducerContext ctx, ulong x)
    {
        Bench.BlackBox(ctx.Db.unique_0_u32_u64_u64.Iter().Where(row => row.x == x));
    }

    [SpacetimeDB.Reducer]
    public static void filter_no_index_u32_u64_u64_by_x(ReducerContext ctx, ulong x)
    {
        Bench.BlackBox(ctx.Db.no_index_u32_u64_u64.Iter().Where(row => row.x == x));
    }

    [SpacetimeDB.Reducer]
    public static void filter_btree_each_column_u32_u64_u64_by_x(ReducerContext ctx, ulong x)
    {
        Bench.BlackBox(ctx.Db.btree_each_column_u32_u64_u64.x.Filter(x));
    }

    [SpacetimeDB.Reducer]
    public static void filter_unique_0_u32_u64_u64_by_y(ReducerContext ctx, ulong y)
    {
        Bench.BlackBox(ctx.Db.unique_0_u32_u64_u64.Iter().Where(row => row.y == y));
    }

    [SpacetimeDB.Reducer]
    public static void filter_no_index_u32_u64_u64_by_y(ReducerContext ctx, ulong y)
    {
        Bench.BlackBox(ctx.Db.no_index_u32_u64_u64.Iter().Where(row => row.y == y));
    }

    [SpacetimeDB.Reducer]
    public static void filter_btree_each_column_u32_u64_u64_by_y(ReducerContext ctx, ulong y)
    {
        Bench.BlackBox(ctx.Db.btree_each_column_u32_u64_u64.y.Filter(y));
    }

    // ---------- delete ----------

    [SpacetimeDB.Reducer]
    public static void delete_unique_0_u32_u64_str_by_id(ReducerContext ctx, uint id)
    {
        ctx.Db.unique_0_u32_u64_str.id.Delete(id);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_0_u32_u64_u64_by_id(ReducerContext ctx, uint id)
    {
        ctx.Db.unique_0_u32_u64_u64.id.Delete(id);
    }

    // ---------- clear table ----------

    [SpacetimeDB.Reducer]
    public static void clear_table_unique_0_u32_u64_str(ReducerContext ctx)
    {
        throw new NotImplementedException("Modules currently have no interface to clear a table");
    }

    [SpacetimeDB.Reducer]
    public static void clear_table_no_index_u32_u64_str(ReducerContext ctx)
    {
        throw new NotImplementedException("Modules currently have no interface to clear a table");
    }

    [SpacetimeDB.Reducer]
    public static void clear_table_btree_each_column_u32_u64_str(ReducerContext ctx)
    {
        throw new NotImplementedException("Modules currently have no interface to clear a table");
    }

    [SpacetimeDB.Reducer]
    public static void clear_table_unique_0_u32_u64_u64(ReducerContext ctx)
    {
        throw new NotImplementedException("Modules currently have no interface to clear a table");
    }

    [SpacetimeDB.Reducer]
    public static void clear_table_no_index_u32_u64_u64(ReducerContext ctx)
    {
        throw new NotImplementedException("Modules currently have no interface to clear a table");
    }

    [SpacetimeDB.Reducer]
    public static void clear_table_btree_each_column_u32_u64_u64(ReducerContext ctx)
    {
        throw new NotImplementedException("Modules currently have no interface to clear a table");
    }

    // ---------- count ----------

    [SpacetimeDB.Reducer]
    public static void count_unique_0_u32_u64_str(ReducerContext ctx)
    {
        Log.Info("COUNT: " + ctx.Db.unique_0_u32_u64_str.Count);
    }

    [SpacetimeDB.Reducer]
    public static void count_no_index_u32_u64_str(ReducerContext ctx)
    {
        Log.Info("COUNT: " + ctx.Db.no_index_u32_u64_str.Count);
    }

    [SpacetimeDB.Reducer]
    public static void count_btree_each_column_u32_u64_str(ReducerContext ctx)
    {
        Log.Info("COUNT: " + ctx.Db.btree_each_column_u32_u64_str.Count);
    }

    [SpacetimeDB.Reducer]
    public static void count_unique_0_u32_u64_u64(ReducerContext ctx)
    {
        Log.Info("COUNT: " + ctx.Db.unique_0_u32_u64_u64.Count);
    }

    [SpacetimeDB.Reducer]
    public static void count_no_index_u32_u64_u64(ReducerContext ctx)
    {
        Log.Info("COUNT: " + ctx.Db.no_index_u32_u64_u64.Count);
    }

    [SpacetimeDB.Reducer]
    public static void count_btree_each_column_u32_u64_u64(ReducerContext ctx)
    {
        Log.Info("COUNT: " + ctx.Db.btree_each_column_u32_u64_u64.Count);
    }

    // ---------- module-specific stuff ----------

    [SpacetimeDB.Reducer]
    public static void fn_with_1_args(ReducerContext ctx, string _arg) { }

    [SpacetimeDB.Reducer]
    public static void fn_with_32_args(
        ReducerContext ctx,
        string _arg1,
        string _arg2,
        string _arg3,
        string _arg4,
        string _arg5,
        string _arg6,
        string _arg7,
        string _arg8,
        string _arg9,
        string _arg10,
        string _arg11,
        string _arg12,
        string _arg13,
        string _arg14,
        string _arg15,
        string _arg16,
        string _arg17,
        string _arg18,
        string _arg19,
        string _arg20,
        string _arg21,
        string _arg22,
        string _arg23,
        string _arg24,
        string _arg25,
        string _arg26,
        string _arg27,
        string _arg28,
        string _arg29,
        string _arg30,
        string _arg31,
        string _arg32
    ) { }

    [SpacetimeDB.Reducer]
    public static void print_many_things(ReducerContext ctx, uint n)
    {
        for (int i = 0; i < n; i++)
        {
            Log.Info("hello again!");
        }
    }
}
