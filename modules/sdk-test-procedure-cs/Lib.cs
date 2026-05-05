namespace SpacetimeDB.Sdk.Test.Procedure;

using SpacetimeDB;

#pragma warning disable STDB_UNSTABLE // Enable experimental SpacetimeDB features

// Types outside Module to avoid naming collision with method "ReturnStruct"
[SpacetimeDB.Type]
public partial struct ReturnStruct
{
    public uint A;
    public string B;
}

[SpacetimeDB.Type]
public partial record ReturnEnum : SpacetimeDB.TaggedEnum<(uint A, string B)>;

public static partial class Module
{
    /// <summary>
    /// Test returning a primitive type
    /// </summary>
    [SpacetimeDB.Procedure]
    public static uint ReturnPrimitive(ProcedureContext ctx, uint lhs, uint rhs)
    {
        return lhs + rhs;
    }

    /// <summary>
    /// Test returning a struct
    /// </summary>
    [SpacetimeDB.Procedure]
    public static ReturnStruct ReturnStruct(ProcedureContext ctx, uint a, string b)
    {
        return new ReturnStruct { A = a, B = b };
    }

    /// <summary>
    /// Test returning enum variant A
    /// </summary>
    [SpacetimeDB.Procedure]
    public static ReturnEnum ReturnEnumA(ProcedureContext ctx, uint a)
    {
        return new ReturnEnum.A(a);
    }

    /// <summary>
    /// Test returning enum variant B
    /// </summary>
    [SpacetimeDB.Procedure]
    public static ReturnEnum ReturnEnumB(ProcedureContext ctx, string b)
    {
        return new ReturnEnum.B(b);
    }

    /// <summary>
    /// Test procedure that panics
    /// </summary>
    [SpacetimeDB.Procedure]
    public static void WillPanic(ProcedureContext ctx)
    {
        throw new Exception("This procedure is expected to panic");
    }

    /// <summary>
    /// Test HTTP GET request to the module's own schema endpoint
    /// </summary>
    [SpacetimeDB.Procedure]
    public static string ReadMySchema(ProcedureContext ctx, string serverUrl)
    {
        var moduleIdentity = ProcedureContextBase.Identity;
        var result = ctx.Http.Get($"{serverUrl}/v1/database/{moduleIdentity}/schema?version=9");
        return result.Match(
            response => response.Body.ToStringUtf8Lossy(),
            error => throw new Exception($"HTTP request failed: {error}")
        );
    }

    /// <summary>
    /// Test HTTP request with invalid URL (should fail gracefully)
    /// </summary>
    [SpacetimeDB.Procedure]
    public static string InvalidRequest(ProcedureContext ctx)
    {
        var result = ctx.Http.Get("http://foo.invalid/");
        return result.Match(
            response => throw new Exception($"Got result from requesting `http://foo.invalid`... huh?\n{response.Body.ToStringUtf8Lossy()}"),
            error => error.Message
        );
    }
    
    [SpacetimeDB.Table(Accessor = "my_table", Public = true)]
    public partial struct MyTable
    {
        public ReturnStruct Field;
    }

    private static void InsertMyTable(ProcedureTxContext ctx)
    {
        ctx.Db.my_table.Insert(new MyTable
        {
            Field = new ReturnStruct
            {
                A = 42,
                B = "magic"
            }
        });
    }

    private static void AssertRowCount(ProcedureContext ctx, ulong expected)
    {
        ctx.WithTx(tx =>
        {
            var actual = tx.Db.my_table.Count;
            if (actual != expected)
            {
                throw new Exception($"Expected {expected} rows but got {actual}");
            }
            return 0;
        });
    }

    /// <summary>
    /// Test transaction that commits
    /// </summary>
    [SpacetimeDB.Procedure]
    public static void InsertWithTxCommit(ProcedureContext ctx)
    {
        // Insert a row and commit
        ctx.WithTx(tx =>
        {
            InsertMyTable(tx);
            return 0;
        });

        // Assert that there's a row
        AssertRowCount(ctx, 1);
    }

    /// <summary>
    /// Test transaction that rolls back
    /// </summary>
    [SpacetimeDB.Procedure]
    public static void InsertWithTxRollback(ProcedureContext ctx)
    {
        // Insert a row then return Err to trigger rollback (matching Rust's try_with_tx pattern)
        ctx.TryWithTx((ProcedureTxContext tx) =>
        {
            InsertMyTable(tx);
            return Result<int, Exception>.Err(new Exception("Rollback"));
        });

        // Assert that there's not a row
        AssertRowCount(ctx, 0);
    }
    
    [SpacetimeDB.Reducer]
    public static void ScheduleProc(ReducerContext ctx)
    {
        // Schedule the procedure to run in 1s
        ctx.Db.scheduled_proc_table.Insert(new ScheduledProcTable
        {
            ScheduledId = 0,
            ScheduledAt = new TimeDuration(1_000_000), // 1 second = 1,000,000 microseconds
            // Store the timestamp at which this reducer was called
            ReducerTs = ctx.Timestamp,
            X = 42,
            Y = 24
        });
    }
    
    [SpacetimeDB.Table(Accessor = "scheduled_proc_table", Scheduled = "ScheduledProc")]
    public partial struct ScheduledProcTable
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong ScheduledId;
        public ScheduleAt ScheduledAt;
        public Timestamp ReducerTs;
        public byte X;
        public byte Y;
    }
    
    [SpacetimeDB.Procedure]
    public static void ScheduledProc(ProcedureContext ctx, ScheduledProcTable data)
    {
        var reducerTs = data.ReducerTs;
        var x = data.X;
        var y = data.Y;
        var procedureTs = ctx.Timestamp;

        ctx.WithTx(tx =>
        {
            tx.Db.proc_inserts_into.Insert(new ProcInsertsInto
            {
                ReducerTs = reducerTs,
                ProcedureTs = procedureTs,
                X = x,
                Y = y
            });
            return 0;
        });
    }
    
    [SpacetimeDB.Table(Accessor = "proc_inserts_into", Public = true)]
    public partial struct ProcInsertsInto
    {
        public Timestamp ReducerTs;
        public Timestamp ProcedureTs;
        public byte X;
        public byte Y;
    }
    
    [SpacetimeDB.Table(Accessor = "pk_uuid", Public = true)]
    public partial struct PkUuid
    {
        public Uuid U;
        public byte Data;
    }
    
    [SpacetimeDB.Procedure]
    public static void SortedUuidsInsert(ProcedureContext ctx)
    {
        ctx.WithTx(tx =>
        {
            // Generate and insert 1000 UUIDs
            for (int i = 0; i < 1000; i++)
            {
                var uuid = ctx.NewUuidV7();
                tx.Db.pk_uuid.Insert(new PkUuid { U = uuid, Data = 0 });
            }

            // Verify UUIDs are sorted
            Uuid? lastUuid = null;
            foreach (var row in tx.Db.pk_uuid.Iter())
            {
                if (lastUuid.HasValue && lastUuid.Value.CompareTo(row.U) >= 0)
                {
                    throw new Exception("UUIDs are not sorted correctly");
                }
                lastUuid = row.U;
            }
            return 0;
        });
    }
}
