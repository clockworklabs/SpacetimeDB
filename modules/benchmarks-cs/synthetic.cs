using SpacetimeDB;

namespace Benchmarks;

public static partial class synthetic
{
    // ---------- schemas ----------

    [SpacetimeDB.Table]
    public partial struct unique_0_u32_u64_str
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public uint id;
        public ulong age;
        public string name;
    }

    [SpacetimeDB.Table]
    public partial struct no_index_u32_u64_str
    {
        public uint id;
        public ulong age;
        public string name;
    }

    [SpacetimeDB.Table]
    public partial struct btree_each_column_u32_u64_str
    {
        [SpacetimeDB.Column(ColumnAttrs.Indexed)]
        public uint id;

        [SpacetimeDB.Column(ColumnAttrs.Indexed)]
        public ulong age;

        [SpacetimeDB.Column(ColumnAttrs.Indexed)]
        public string name;
    }

    [SpacetimeDB.Table]
    public partial struct unique_0_u32_u64_u64
    {
        [SpacetimeDB.Column(ColumnAttrs.Unique)]
        public uint id;
        public ulong x;
        public ulong y;
    }

    [SpacetimeDB.Table]
    public partial struct no_index_u32_u64_u64
    {
        public uint id;
        public ulong x;
        public ulong y;
    }

    [SpacetimeDB.Table]
    public partial struct btree_each_column_u32_u64_u64
    {
        [SpacetimeDB.Column(ColumnAttrs.Indexed)]
        public uint id;

        [SpacetimeDB.Column(ColumnAttrs.Indexed)]
        public ulong x;

        [SpacetimeDB.Column(ColumnAttrs.Indexed)]
        public ulong y;
    }

    // ---------- empty ----------

    [SpacetimeDB.Reducer]
    public static void empty() { }

    // ---------- insert ----------

    [SpacetimeDB.Reducer]
    public static void insert_unique_0_u32_u64_str(uint id, ulong age, string name)
    {
        new unique_0_u32_u64_str()
        {
            id = id,
            age = age,
            name = name,
        }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void insert_no_index_u32_u64_str(uint id, ulong age, string name)
    {
        new no_index_u32_u64_str()
        {
            id = id,
            age = age,
            name = name,
        }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void insert_btree_each_column_u32_u64_str(uint id, ulong age, string name)
    {
        new btree_each_column_u32_u64_str()
        {
            id = id,
            age = age,
            name = name,
        }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void insert_unique_0_u32_u64_u64(uint id, ulong x, ulong y)
    {
        new unique_0_u32_u64_u64()
        {
            id = id,
            x = x,
            y = y,
        }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void insert_no_index_u32_u64_u64(uint id, ulong x, ulong y)
    {
        new no_index_u32_u64_u64()
        {
            id = id,
            x = x,
            y = y,
        }.Insert();
    }

    [SpacetimeDB.Reducer]
    public static void insert_btree_each_column_u32_u64_u64(uint id, ulong x, ulong y)
    {
        new btree_each_column_u32_u64_u64()
        {
            id = id,
            x = x,
            y = y,
        }.Insert();
    }

    // ---------- insert bulk ----------

    [SpacetimeDB.Reducer]
    public static void insert_bulk_unique_0_u32_u64_u64(List<unique_0_u32_u64_u64> locs)
    {
        foreach (unique_0_u32_u64_u64 loc in locs)
        {
            loc.Insert();
        }
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_no_index_u32_u64_u64(List<no_index_u32_u64_u64> locs)
    {
        foreach (no_index_u32_u64_u64 loc in locs)
        {
            loc.Insert();
        }
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_btree_each_column_u32_u64_u64(
        List<btree_each_column_u32_u64_u64> locs
    )
    {
        foreach (btree_each_column_u32_u64_u64 loc in locs)
        {
            loc.Insert();
        }
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_unique_0_u32_u64_str(List<unique_0_u32_u64_str> people)
    {
        foreach (unique_0_u32_u64_str u32_u64_str in people)
        {
            u32_u64_str.Insert();
        }
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_no_index_u32_u64_str(List<no_index_u32_u64_str> people)
    {
        foreach (no_index_u32_u64_str u32_u64_str in people)
        {
            u32_u64_str.Insert();
        }
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_btree_each_column_u32_u64_str(
        List<btree_each_column_u32_u64_str> people
    )
    {
        foreach (btree_each_column_u32_u64_str u32_u64_str in people)
        {
            u32_u64_str.Insert();
        }
    }

    // ---------- update ----------

    [SpacetimeDB.Reducer]
    public static void update_bulk_unique_0_u32_u64_u64(uint rowCount)
    {
        int hit = 0;
        foreach (unique_0_u32_u64_u64 loc in unique_0_u32_u64_u64.Iter().Take((int)rowCount))
        {
            hit++;
            unique_0_u32_u64_u64.UpdateByid(
                loc.id,
                new()
                {
                    id = loc.id,
                    x = loc.x + 1,
                    y = loc.y,
                }
            );
        }
        if (hit != rowCount)
        {
            throw new Exception("Not enough rows to perform requested amount of updates");
        }
    }

    [SpacetimeDB.Reducer]
    public static void update_bulk_unique_0_u32_u64_str(uint rowCount)
    {
        uint hit = 0;
        foreach (
            unique_0_u32_u64_str u32_u64_str in unique_0_u32_u64_str.Iter().Take((int)rowCount)
        )
        {
            hit++;
            unique_0_u32_u64_str.UpdateByid(
                u32_u64_str.id,
                new()
                {
                    id = u32_u64_str.id,
                    name = u32_u64_str.name,
                    age = u32_u64_str.age + 1,
                }
            );
        }
        if (hit != rowCount)
        {
            throw new Exception("Not enough rows to perform requested amount of updates");
        }
    }

    // ---------- iterate ----------

    [SpacetimeDB.Reducer]
    public static void iterate_unique_0_u32_u64_str()
    {
        foreach (unique_0_u32_u64_str u32_u64_str in unique_0_u32_u64_str.Iter())
        {
            Bench.BlackBox(u32_u64_str);
        }
    }

    [SpacetimeDB.Reducer]
    public static void iterate_unique_0_u32_u64_u64()
    {
        foreach (unique_0_u32_u64_u64 u32_u64_u64 in unique_0_u32_u64_u64.Iter())
        {
            Bench.BlackBox(u32_u64_u64);
        }
    }

    // ---------- filtering ----------

    [SpacetimeDB.Reducer]
    public static void filter_unique_0_u32_u64_str_by_id(uint id)
    {
        Bench.BlackBox(unique_0_u32_u64_str.FindByid(id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_no_index_u32_u64_str_by_id(uint id)
    {
        Bench.BlackBox(no_index_u32_u64_str.FilterByid(id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_btree_each_column_u32_u64_str_by_id(uint id)
    {
        Bench.BlackBox(btree_each_column_u32_u64_str.FilterByid(id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_unique_0_u32_u64_str_by_name(string name)
    {
        Bench.BlackBox(unique_0_u32_u64_str.FilterByname(name));
    }

    [SpacetimeDB.Reducer]
    public static void filter_no_index_u32_u64_str_by_name(string name)
    {
        Bench.BlackBox(no_index_u32_u64_str.FilterByname(name));
    }

    [SpacetimeDB.Reducer]
    public static void filter_btree_each_column_u32_u64_str_by_name(string name)
    {
        Bench.BlackBox(btree_each_column_u32_u64_str.FilterByname(name));
    }

    [SpacetimeDB.Reducer]
    public static void filter_unique_0_u32_u64_u64_by_id(uint id)
    {
        Bench.BlackBox(unique_0_u32_u64_u64.FindByid(id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_no_index_u32_u64_u64_by_id(uint id)
    {
        Bench.BlackBox(no_index_u32_u64_u64.FilterByid(id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_btree_each_column_u32_u64_u64_by_id(uint id)
    {
        Bench.BlackBox(btree_each_column_u32_u64_u64.FilterByid(id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_unique_0_u32_u64_u64_by_x(ulong x)
    {
        Bench.BlackBox(unique_0_u32_u64_u64.FilterByx(x));
    }

    [SpacetimeDB.Reducer]
    public static void filter_no_index_u32_u64_u64_by_x(ulong x)
    {
        Bench.BlackBox(no_index_u32_u64_u64.FilterByx(x));
    }

    [SpacetimeDB.Reducer]
    public static void filter_btree_each_column_u32_u64_u64_by_x(ulong x)
    {
        Bench.BlackBox(btree_each_column_u32_u64_u64.FilterByx(x));
    }

    [SpacetimeDB.Reducer]
    public static void filter_unique_0_u32_u64_u64_by_y(ulong x)
    {
        Bench.BlackBox(unique_0_u32_u64_u64.FilterByy(x));
    }

    [SpacetimeDB.Reducer]
    public static void filter_no_index_u32_u64_u64_by_y(ulong x)
    {
        Bench.BlackBox(no_index_u32_u64_u64.FilterByy(x));
    }

    [SpacetimeDB.Reducer]
    public static void filter_btree_each_column_u32_u64_u64_by_y(ulong x)
    {
        Bench.BlackBox(btree_each_column_u32_u64_u64.FilterByy(x));
    }

    // ---------- delete ----------

    [SpacetimeDB.Reducer]
    public static void delete_unique_0_u32_u64_str_by_id(uint id)
    {
        unique_0_u32_u64_str.DeleteByid(id);
    }

    [SpacetimeDB.Reducer]
    public static void delete_unique_0_u32_u64_u64_by_id(uint id)
    {
        unique_0_u32_u64_u64.DeleteByid(id);
    }

    // ---------- clear table ----------

    [SpacetimeDB.Reducer]
    public static void clear_table_unique_0_u32_u64_str()
    {
        throw new NotImplementedException("Modules currently have no interface to clear a table");
    }

    [SpacetimeDB.Reducer]
    public static void clear_table_no_index_u32_u64_str()
    {
        throw new NotImplementedException("Modules currently have no interface to clear a table");
    }

    [SpacetimeDB.Reducer]
    public static void clear_table_btree_each_column_u32_u64_str()
    {
        throw new NotImplementedException("Modules currently have no interface to clear a table");
    }

    [SpacetimeDB.Reducer]
    public static void clear_table_unique_0_u32_u64_u64()
    {
        throw new NotImplementedException("Modules currently have no interface to clear a table");
    }

    [SpacetimeDB.Reducer]
    public static void clear_table_no_index_u32_u64_u64()
    {
        throw new NotImplementedException("Modules currently have no interface to clear a table");
    }

    [SpacetimeDB.Reducer]
    public static void clear_table_btree_each_column_u32_u64_u64()
    {
        throw new NotImplementedException("Modules currently have no interface to clear a table");
    }

    // ---------- count ----------

    [SpacetimeDB.Reducer]
    public static void count_unique_0_u32_u64_str()
    {
        Runtime.Log("COUNT: " + unique_0_u32_u64_str.Iter().Count());
    }

    [SpacetimeDB.Reducer]
    public static void count_no_index_u32_u64_str()
    {
        Runtime.Log("COUNT: " + no_index_u32_u64_str.Iter().Count());
    }

    [SpacetimeDB.Reducer]
    public static void count_btree_each_column_u32_u64_str()
    {
        Runtime.Log("COUNT: " + btree_each_column_u32_u64_str.Iter().Count());
    }

    [SpacetimeDB.Reducer]
    public static void count_unique_0_u32_u64_u64()
    {
        Runtime.Log("COUNT: " + unique_0_u32_u64_u64.Iter().Count());
    }

    [SpacetimeDB.Reducer]
    public static void count_no_index_u32_u64_u64()
    {
        Runtime.Log("COUNT: " + no_index_u32_u64_u64.Iter().Count());
    }

    [SpacetimeDB.Reducer]
    public static void count_btree_each_column_u32_u64_u64()
    {
        Runtime.Log("COUNT: " + btree_each_column_u32_u64_u64.Iter().Count());
    }

    // ---------- module-specific stuff ----------

    [SpacetimeDB.Reducer]
    public static void fn_with_1_args(string arg) { }

    [SpacetimeDB.Reducer]
    public static void fn_with_32_args(
        string arg1,
        string arg2,
        string arg3,
        string arg4,
        string arg5,
        string arg6,
        string arg7,
        string arg8,
        string arg9,
        string arg10,
        string arg11,
        string arg12,
        string arg13,
        string arg14,
        string arg15,
        string arg16,
        string arg17,
        string arg18,
        string arg19,
        string arg20,
        string arg21,
        string arg22,
        string arg23,
        string arg24,
        string arg25,
        string arg26,
        string arg27,
        string arg28,
        string arg29,
        string arg30,
        string arg31,
        string arg32
    )
    { }

    [SpacetimeDB.Reducer]
    public static void print_many_things(uint n)
    {
        for (int i = 0; i < n; i++)
        {
            Runtime.Log("hello again!");
        }
    }
}
