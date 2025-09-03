using Bogus;
using SpacetimeDB;

namespace Benchmarks;

[SpacetimeDB.Type]
public enum BenchLoad
{
    Tiny,
    Small,
    Medium,
    Large
}

[SpacetimeDB.Type]
public enum Index
{
    One,
    Many
}

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
    public partial struct btree_each_column_u32_u64_str_t
    {
        [SpacetimeDB.Index.BTree]
        public uint id;

        [SpacetimeDB.Index.BTree]
        public ulong age;

        [SpacetimeDB.Index.BTree]
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
    public partial struct btree_each_column_u32_u64_u64_t
    {
        [SpacetimeDB.Index.BTree]
        public uint id;

        [SpacetimeDB.Index.BTree]
        public ulong x;

        [SpacetimeDB.Index.BTree]
        public ulong y;
    }

    [SpacetimeDB.Table(Name = "tiny_rows")]
    public partial struct tiny_rows_t
    {
        [SpacetimeDB.Index.BTree]
        public byte id;
    }

    [SpacetimeDB.Table(Name = "small_rows")]
    public partial struct small_rows_t
    {
        [SpacetimeDB.Index.BTree]
        public ulong id;
        public ulong x;
        public ulong y;
    }

    [SpacetimeDB.Table(Name = "small_btree_each_column_rows")]
    public partial struct small_rows_btree_each_column_t
    {
        [SpacetimeDB.Index.BTree]
        public ulong id;
        [SpacetimeDB.Index.BTree]
        public ulong x;
        [SpacetimeDB.Index.BTree]
        public ulong y;
    }

    [SpacetimeDB.Table(Name = "medium_var_rows")]
    public partial struct medium_var_rows_t
    {
        [SpacetimeDB.Index.BTree]
        public ulong id;
        public string name;
        public string email;
        public string password;
        public Identity identity;
        public ConnectionId connection;
        public List<ulong> pos;
    }

    [SpacetimeDB.Table(Name = "medium_var_rows_btree_each_column")]
    public partial struct medium_var_rows_btree_each_column_t
    {
        [SpacetimeDB.Index.BTree]
        public ulong id;
        [SpacetimeDB.Index.BTree]
        public string name;
        [SpacetimeDB.Index.BTree]
        public string email;
        [SpacetimeDB.Index.BTree]
        public string password;
        [SpacetimeDB.Index.BTree]
        public Identity identity;
        [SpacetimeDB.Index.BTree]
        public ConnectionId connection;
        //[SpacetimeDB.Index.BTree]: Not supported yet on C#
        public List<ulong> pos;
    }

    [SpacetimeDB.Table(Name = "large_var_rows")]
    public partial struct large_var_rows_t
    {
        [SpacetimeDB.Index.BTree]
        public UInt128 id;
        public string invoice_code;
        public string status;
        public Identity customer;
        public ConnectionId company;
        public string user_name;

        public double price;
        public double cost;
        public double discount;
        public List<double> taxes;
        public double tax_total;
        public double sub_total;
        public double total;

        public string country;
        public string state;
        public string city;
        public string zip_code;
        public string phone;

        public string notes;
        public List<string>? tags;
    }

    [SpacetimeDB.Table(Name = "large_var_rows_btree_each_column")]
    public partial struct large_var_rows_btree_each_column_t
    {
        [SpacetimeDB.Index.BTree]
        public UInt128 id;
        [SpacetimeDB.Index.BTree]
        public string invoice_code;
        [SpacetimeDB.Index.BTree]
        public string status;
        [SpacetimeDB.Index.BTree]
        public Identity customer;
        [SpacetimeDB.Index.BTree]
        public ConnectionId company;
        [SpacetimeDB.Index.BTree]
        public string user_name;

        [SpacetimeDB.Index.BTree]
        public double price;
        [SpacetimeDB.Index.BTree]
        public double cost;
        [SpacetimeDB.Index.BTree]
        public double discount;
        //[SpacetimeDB.Index.BTree]: Not supported yet on C#
        public List<double> taxes;
        [SpacetimeDB.Index.BTree]
        public double tax_total;
        [SpacetimeDB.Index.BTree]
        public double sub_total;
        [SpacetimeDB.Index.BTree]
        public double total;

        [SpacetimeDB.Index.BTree]
        public string country;
        [SpacetimeDB.Index.BTree]
        public string state;
        [SpacetimeDB.Index.BTree]
        public string city;
        [SpacetimeDB.Index.BTree]
        public string zip_code;
        [SpacetimeDB.Index.BTree]
        public string phone;
        [SpacetimeDB.Index.BTree]

        public string notes;
        //[SpacetimeDB.Index.BTree]: Not supported yet on C#
        public List<string>? tags;
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

    public static Identity rand_identity(Faker fake)
    {
        return new Identity(fake.Random.Bytes(32));
    }

    public static ConnectionId rand_connection_id(Faker fake)
    {
        return ConnectionId.Random();
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_tiny_rows(ReducerContext ctx, byte rows)
    {
        for (byte id = 0; id < rows; id++)
        {
            ctx.Db.tiny_rows.Insert(new tiny_rows_t { id = id });
        }
        Log.Info($"Inserted on tiny_rows: {rows} rows");
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_small_rows(ReducerContext ctx, ulong rows)
    {
        var rng = new Random();
        for (ulong id = 0; id < rows; id++)
        {
            ctx.Db.small_rows.Insert(new small_rows_t
            {
                id = id,
                x = (ulong)rng.Next(),
                y = (ulong)rng.Next()
            });
        }
        Log.Info($"Inserted on small_rows: {rows} rows");
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_small_btree_each_column_rows(ReducerContext ctx, ulong rows)
    {
        var rng = new Random();
        for (ulong id = 0; id < rows; id++)
        {
            ctx.Db.small_btree_each_column_rows.Insert(new small_rows_btree_each_column_t
            {
                id = id,
                x = (ulong)rng.Next(),
                y = (ulong)rng.Next()
            });
        }
        Log.Info($"Inserted on small_btree_each_column_rows: {rows} rows");
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_medium_var_rows(ReducerContext ctx, ulong rows)
    {
        var faker = new Faker("en");
        for (ulong id = 0; id < rows; id++)
        {
            ctx.Db.medium_var_rows.Insert(new medium_var_rows_t
            {
                id = id,
                name = faker.Name.FullName(),
                email = faker.Internet.Email(),
                password = faker.Internet.Password(),
                identity = rand_identity(faker),
                connection = rand_connection_id(faker),
                pos = new Faker<List<ulong>>().Generate()
            });
        }
        Log.Info($"Inserted on medium_var_rows: {rows} rows");
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_medium_var_rows_btree_each_column(ReducerContext ctx, ulong rows)
    {
        var faker = new Faker("en");
        for (ulong id = 0; id < rows; id++)
        {
            ctx.Db.medium_var_rows_btree_each_column.Insert(new medium_var_rows_btree_each_column_t
            {
                id = id,
                name = faker.Name.FullName(),
                email = faker.Internet.Email(),
                password = faker.Internet.Password(length: 10),
                identity = rand_identity(faker),
                connection = rand_connection_id(faker),
                pos = new Faker<List<ulong>>().Generate()
            });
        }
        Log.Info($"Inserted on medium_var_rows_btree_each_column: {rows} rows");
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_large_var_rows(ReducerContext ctx, ulong rows)
    {
        var faker = new Faker("en");
        for (ulong id = 0; id < rows; id++)
        {
            ctx.Db.large_var_rows.Insert(new large_var_rows_t
            {
                id = UInt128.CreateChecked<ulong>(id),
                invoice_code = faker.Random.String(),
                status = faker.Random.String(),
                customer = rand_identity(faker),
                company = rand_connection_id(faker),
                user_name = faker.Random.String(),

                price = faker.Random.Double(),
                cost = faker.Random.Double(),
                discount = faker.Random.Double(),
                taxes = new Faker<List<double>>().Generate(),
                tax_total = faker.Random.Double(),
                sub_total = faker.Random.Double(),
                total = faker.Random.Double(),

                country = faker.Address.Country(),
                state = faker.Address.State(),
                city = faker.Address.City(),
                zip_code = faker.Address.ZipCode(),
                phone = faker.Phone.PhoneNumber(),
                notes = faker.Lorem.Paragraph(),
                tags = new Faker<string>().GenerateBetween(min: 0, max: 3)
            });
        }
        Log.Info($"Inserted on large_var_rows: {rows} rows");
    }

    [SpacetimeDB.Reducer]
    public static void insert_bulk_large_var_rows_btree_each_column(ReducerContext ctx, ulong rows)
    {
        var faker = new Faker("en");
        for (ulong id = 0; id < rows; id++)
        {
            ctx.Db.large_var_rows_btree_each_column.Insert(new large_var_rows_btree_each_column_t
            {
                id = UInt128.CreateChecked<ulong>(id),
                invoice_code = faker.Random.String(),
                status = faker.Random.String(),
                customer = rand_identity(faker),
                company = rand_connection_id(faker),
                user_name = faker.Random.String(),

                price = faker.Random.Double(),
                cost = faker.Random.Double(),
                discount = faker.Random.Double(),
                taxes = new Faker<List<double>>().Generate(),
                tax_total = faker.Random.Double(),
                sub_total = faker.Random.Double(),
                total = faker.Random.Double(),

                country = faker.Address.Country(),
                state = faker.Address.State(),
                city = faker.Address.City(),
                zip_code = faker.Address.ZipCode(),
                phone = faker.Phone.PhoneNumber(),
                notes = faker.Lorem.Paragraph(),
                tags = [.. faker.Random.WordsArray(0,3)]

            });
        }
        Log.Info($"Inserted on large_var_rows_btree_each_column: {rows} rows");
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

    [SpacetimeDB.Reducer]
    public static void filter_tiny_rows_by_id(ReducerContext ctx, byte id)
    {
        Bench.BlackBox(ctx.Db.tiny_rows.Iter().Where(row => row.id == id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_small_rows_by_id(ReducerContext ctx, ulong id)
    {
        Bench.BlackBox(ctx.Db.small_rows.Iter().Where(row => row.id == id));

    }
    
    [SpacetimeDB.Reducer]
    public static void filter_medium_var_rows_by_id(ReducerContext ctx, ulong id)
    {
        Bench.BlackBox(ctx.Db.medium_var_rows.Iter().Where(row => row.id == id));
    }

    [SpacetimeDB.Reducer]
    public static void filter_large_var_rows_by_id(ReducerContext ctx, ulong id)
    {
        Bench.BlackBox(ctx.Db.large_var_rows.Iter().Where(row => row.id == id));
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

    [SpacetimeDB.Reducer]
    public static void delete_tiny_rows_by_id(ReducerContext ctx, byte id)
    {
        ctx.Db.tiny_rows.id.Delete(id);
    }

    [SpacetimeDB.Reducer]
    public static void delete_small_rows_by_id(ReducerContext ctx, ulong id)
    {
        ctx.Db.small_rows.id.Delete(id);
    }

    [SpacetimeDB.Reducer]
    public static void delete_medium_var_rows_by_id(ReducerContext ctx, ulong id)
    {
        ctx.Db.medium_var_rows.id.Delete(id);
    }

    [SpacetimeDB.Reducer]
    public static void delete_large_var_rows_by_id(ReducerContext ctx, ulong id)
    {
        ctx.Db.large_var_rows.id.Delete(id);
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
    )
    { }

    [SpacetimeDB.Reducer]
    public static void print_many_things(ReducerContext ctx, uint n)
    {
        for (int i = 0; i < n; i++)
        {
            Log.Info("hello again!");
        }
    }

    private static BenchLoad ParseLoad(string arg)
    {
        var load = arg switch
        {
            "tiny" => BenchLoad.Tiny,
            "small" => BenchLoad.Small,
            "medium" => BenchLoad.Medium,
            "large" => BenchLoad.Large,
            _ => throw new Exception($"Invalid load type: '{arg}', expected: tiny, small, medium, or large"),
        };
        return load;
    }

    /// This reducer is used to load synthetic data into the database for benchmarking purposes.
    ///
    /// The input is a string with the following format:
    ///
    /// `load_type`: [`Load`], `index_type`: [`Index`], `row_count`: `uint`
    [SpacetimeDB.Reducer]
    public static void load(ReducerContext ctx, string input)
    {
        var args = input.Split(',').Select(x => x.Trim().ToLower()).ToList();
        if (args.Count != 3)
        {
            throw new Exception($"Expected 3 arguments, got {args.Count}");
        }
        var load = ParseLoad(args[0]);

        var index = args[1] switch
        {
            "one" => Index.One,
            "many" => Index.Many,
            _ => throw new Exception($"Invalid index type: '{args[1]}', expected: one, or many"),
        };
        if (!ulong.TryParse(args[2], out var rows))
        {
            throw new Exception($"Invalid row count: {args[2]}");
        }

        switch (load)
        {
            case BenchLoad.Tiny when index == Index.One || index == Index.Many:
                insert_bulk_tiny_rows(ctx, (byte)rows);
                break;
            case BenchLoad.Small when index == Index.One:
                insert_bulk_small_rows(ctx, rows);
                break;
            case BenchLoad.Small when index == Index.Many:
                insert_bulk_small_btree_each_column_rows(ctx, rows);
                break;
            case BenchLoad.Medium when index == Index.One:
                insert_bulk_medium_var_rows(ctx, rows);
                break;
            case BenchLoad.Medium when index == Index.Many:
                insert_bulk_medium_var_rows_btree_each_column(ctx, rows);
                break;
            case BenchLoad.Large when index == Index.One:
                insert_bulk_large_var_rows(ctx, rows);
                break;
            case BenchLoad.Large when index == Index.Many:
                insert_bulk_large_var_rows_btree_each_column(ctx, rows);
                break;
        }
    }

    /// Used to execute a series of reducers in sequence for benchmarking purposes.
    ///
    /// The input is a string with the following format:
    ///
    /// `load_type`: [`Load`], `inserts`: `u32`, `queries`: `u32`, `deletes`: `u32`
    ///
    /// The order of the `inserts`, `queries`, and `deletes` can be changed and will be executed in that order.
    [SpacetimeDB.Reducer]
    public static void queries(ReducerContext ctx, string input)
    {
        var args = input.Split(',').Select(x => x.Trim().ToLower()).ToList();
        if (args.Count < 2)
        {
            throw new ArgumentException($"Expected at least 2 arguments, got {args.Count}");
        }

        var load = ParseLoad(args[0]);

        ulong inserts = 0, queries = 0, deletes = 0;

        foreach (var arg in args.Skip(1))
        {
            var parts = arg.Split(':').Select(x => x.Trim()).ToList();
            if (parts.Count != 2)
            {
                throw new ArgumentException($"Invalid argument: '{arg}', expected: 'operation:count'");
            }

            if (!ulong.TryParse(parts[1], out var count))
            {
                throw new ArgumentException($"Invalid count: {parts[1]}");
            }

            switch (parts[0])
            {
                case "inserts":
                    inserts = count;
                    break;
                case "queries":
                    queries = count;
                    break;
                case "deletes":
                    deletes = count;
                    break;
                default:
                    throw new ArgumentException($"Invalid operation: '{parts[0]}', expected: inserts, queries, or deletes");
            }
        }

        Log.Info($"Executing queries: inserts: {inserts}, queries: {queries}, deletes: {deletes}");

        switch (load)
        {
            case BenchLoad.Tiny:
                if (inserts > 0) insert_bulk_tiny_rows(ctx, (byte)inserts);
                for (ulong id = 0; id < queries; id++) filter_tiny_rows_by_id(ctx, (byte)id);
                for (ulong id = 0; id < deletes; id++) delete_tiny_rows_by_id(ctx, (byte)id);
                break;
            case BenchLoad.Small:
                if (inserts > 0) insert_bulk_small_rows(ctx, inserts);
                for (ulong id = 0; id < queries; id++) filter_small_rows_by_id(ctx, id);
                for (ulong id = 0; id < deletes; id++) delete_small_rows_by_id(ctx, id);
                break;
            case BenchLoad.Medium:
                if (inserts > 0) insert_bulk_medium_var_rows(ctx, inserts);
                for (ulong id = 0; id < queries; id++) filter_medium_var_rows_by_id(ctx, id);
                for (ulong id = 0; id < deletes; id++) delete_medium_var_rows_by_id(ctx, id);
                break;
            case BenchLoad.Large:
                if (inserts > 0) insert_bulk_large_var_rows(ctx, inserts);
                for (ulong id = 0; id < queries; id++) filter_large_var_rows_by_id(ctx, id);
                for (ulong id = 0; id < deletes; id++) delete_large_var_rows_by_id(ctx, id);
                break;
        }
    }
}
