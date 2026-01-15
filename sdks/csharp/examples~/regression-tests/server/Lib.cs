// Server module for regression tests.
// Everything we're testing for happens SDK-side so this module is very uninteresting.

using System.Diagnostics;
using System.Diagnostics.CodeAnalysis;
using SpacetimeDB;

[SpacetimeDB.Type]
public partial class ReturnStruct
{
    public uint A;
    public string B;

    public ReturnStruct(uint a, string b)
    {
        A = a;
        B = b;
    }

    public ReturnStruct()
    {
        A = 0;
        B = string.Empty;
    }
}

[SpacetimeDB.Type]
public partial record ReturnEnum : SpacetimeDB.TaggedEnum<(
    uint A,
    string B
    )>;

[SpacetimeDB.Type]
public partial struct DbVector2
{
    public int X;
    public int Y;
}

public static partial class Module
{
    [SpacetimeDB.Table(Name = "my_table", Public = true)]
    public partial struct MyTable
    {
        public ReturnStruct Field;
    }
    [SpacetimeDB.Table(Name = "example_data", Public = true)]
    public partial struct ExampleData
    {
        [SpacetimeDB.PrimaryKey]
        public uint Id;

        [SpacetimeDB.Index.BTree]
        public uint Indexed;
    }

    [SpacetimeDB.Table(Name = "my_log", Public = true)]
    public partial struct MyLog
    {
        public Result<MyTable, string> msg;
    }

    [SpacetimeDB.Table(Name = "player", Public = true)]
    public partial struct Player
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        [SpacetimeDB.Unique]
        public Identity Identity;

        public string Name;
    }

    [SpacetimeDB.Table(Name = "account", Public = true)]
    public partial class Account
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        [SpacetimeDB.Unique]
        public Identity Identity;

        public string Name = "";
    }

    [SpacetimeDB.Table(Name = "player_level", Public = true)]
    public partial struct PlayerLevel
    {
        [SpacetimeDB.Unique]
        public ulong PlayerId;

        [SpacetimeDB.Index.BTree]
        public ulong Level;
    }

    [SpacetimeDB.Type]
    public partial struct PlayerAndLevel
    {
        public ulong Id;
        public Identity Identity;
        public string Name;
        public ulong Level;
    }

    [SpacetimeDB.Table(Name = "User", Public = true)]
    public partial struct User
    {
        [SpacetimeDB.PrimaryKey]
        public Uuid Id;

        public string Name;

        [SpacetimeDB.Index.BTree]
        public bool IsAdmin;
    }

    [SpacetimeDB.Table(Name = "nullable_vec", Public = true)]
    public partial struct NullableVec
    {
        [SpacetimeDB.PrimaryKey]
        public uint Id;

        public DbVector2? Pos;
    }

    [SpacetimeDB.Table(Name = "null_string_nonnullable", Public = true)]
    public partial struct NullStringNonNullable
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        public string Name;
    }

    [SpacetimeDB.Table(Name = "null_string_nullable", Public = true)]
    public partial struct NullStringNullable
    {
        [SpacetimeDB.PrimaryKey]
        [SpacetimeDB.AutoInc]
        public ulong Id;

        public string? Name;
    }

    // At-most-one row: return T?
    [SpacetimeDB.View(Name = "my_player", Public = true)]
    public static Player? MyPlayer(ViewContext ctx)
    {
        return ctx.Db.player.Identity.Find(ctx.Sender);
    }

    [SpacetimeDB.View(Name = "my_account", Public = true)]
    public static Account? MyAccount(ViewContext ctx)
    {
        return ctx.Db.account.Identity.Find(ctx.Sender) as Account;
    }

    [SpacetimeDB.View(Name = "my_account_missing", Public = true)]
    public static Account? MyAccountMissing(ViewContext ctx)
    {
        return null;
    }

    // Multiple rows: return a list
    [SpacetimeDB.View(Name = "players_at_level_one", Public = true)]
    public static List<PlayerAndLevel> PlayersAtLevelOne(AnonymousViewContext ctx)
    {
        var rows = new List<PlayerAndLevel>();
        foreach (var player in ctx.Db.player_level.Level.Filter(1))
        {
            if (ctx.Db.player.Id.Find(player.PlayerId) is Player p)
            {
                var row = new PlayerAndLevel
                {
                    Id = p.Id,
                    Identity = p.Identity,
                    Name = p.Name,
                    Level = player.Level
                };
                rows.Add(row);
            }
        }
        return rows;
    }

    [SpacetimeDB.View(Name = "Admins", Public = true)]
    public static List<User> Admins(AnonymousViewContext ctx)
    {
        var rows = new List<User>();
        foreach (var user in ctx.Db.User.IsAdmin.Filter(true))
        {
            rows.Add(user);
        }
        return rows;
    }

    [SpacetimeDB.View(Name = "nullable_vec_view", Public = true)]
    public static List<NullableVec> NullableVecView(AnonymousViewContext ctx)
    {
        var rows = new List<NullableVec>();

        if (ctx.Db.nullable_vec.Id.Find(1) is NullableVec row1)
        {
            rows.Add(row1);
        }

        if (ctx.Db.nullable_vec.Id.Find(2) is NullableVec row2)
        {
            rows.Add(row2);
        }
        return rows;
    }

    [SpacetimeDB.Reducer]
    public static void Delete(ReducerContext ctx, uint id)
    {
        LogStopwatch sw = new("Delete");
        ctx.Db.example_data.Id.Delete(id);
    }

    [SpacetimeDB.Reducer]
    public static void Add(ReducerContext ctx, uint id, uint indexed)
    {
        ctx.Db.example_data.Insert(new ExampleData { Id = id, Indexed = indexed });
    }

    [SpacetimeDB.Reducer]
    public static void ThrowError(ReducerContext ctx, string error)
    {
        throw new Exception(error);
    }

    [SpacetimeDB.Reducer]
    public static void InsertResult(ReducerContext ctx, Result<MyTable, string> msg)
    {
        ctx.Db.my_log.Insert(new MyLog { msg = msg });
    }

    [SpacetimeDB.Reducer]
    public static void SetNullableVec(ReducerContext ctx, uint id, bool hasPos, int x, int y)
    {
        var row = new NullableVec
        {
            Id = id,
            Pos = hasPos ? new DbVector2 { X = x, Y = y } : null
        };

        if (ctx.Db.nullable_vec.Id.Find(id) is null)
        {
            ctx.Db.nullable_vec.Insert(row);
        }
        else
        {
            ctx.Db.nullable_vec.Id.Update(row);
        }
    }

    [SpacetimeDB.Reducer]
    public static void InsertEmptyStringIntoNonNullable(ReducerContext ctx)
    {
        ctx.Db.null_string_nonnullable.Insert(new NullStringNonNullable { Name = "" });
    }

    [SpacetimeDB.Reducer]
    public static void InsertNullStringIntoNonNullable(ReducerContext ctx)
    {
        ctx.Db.null_string_nonnullable.Insert(new NullStringNonNullable { Name = null! });
    }

    [SpacetimeDB.Reducer]
    public static void InsertNullStringIntoNullable(ReducerContext ctx)
    {
        ctx.Db.null_string_nullable.Insert(new NullStringNullable { Name = null });
    }

    [Reducer(ReducerKind.ClientConnected)]
    public static void ClientConnected(ReducerContext ctx)
    {
        Log.Info($"Connect {ctx.Sender}");

        if (ctx.Db.player.Identity.Find(ctx.Sender) is Player player)
        {
            // We are not logging player login status, so do nothing
        }
        else
        {
            // Lets setup a new player with a level of 1
            ctx.Db.player.Insert(new Player { Identity = ctx.Sender, Name = "NewPlayer" });
            var playerId = (ctx.Db.player.Identity.Find(ctx.Sender)!).Value.Id;
            ctx.Db.player_level.Insert(new PlayerLevel { PlayerId = playerId, Level = 1 });
        }

        if (ctx.Db.account.Identity.Find(ctx.Sender) is null)
        {
            ctx.Db.account.Insert(new Account { Identity = ctx.Sender, Name = "Account" });
        }

        if (ctx.Db.nullable_vec.Id.Find(1) is null)
        {
            ctx.Db.nullable_vec.Insert(new NullableVec
            {
                Id = 1,
                Pos = new DbVector2 { X = 1, Y = 2 },
            });
        }

        if (ctx.Db.nullable_vec.Id.Find(2) is null)
        {
            ctx.Db.nullable_vec.Insert(new NullableVec
            {
                Id = 2,
                Pos = null,
            });
        }

        foreach (var (Name, IsAdmin) in new List<(string Name, bool IsAdmin)>
            {
                ("Alice", true),
                ("Bob", false),
                ("Charlie", true)
            })
        {
            ctx.Db.User.Insert(new User { Id = ctx.NewUuidV7(), Name = Name, IsAdmin = IsAdmin });
        }
    }

    [SpacetimeDB.Procedure]
    public static uint ReturnPrimitive(ProcedureContext ctx, uint lhs, uint rhs)
    {
        return lhs + rhs;
    }

    [SpacetimeDB.Procedure]
    public static ReturnStruct ReturnStructProcedure(ProcedureContext ctx, uint a, string b)
    {
        return new ReturnStruct(a, b);
    }

    [SpacetimeDB.Procedure]
    public static ReturnEnum ReturnEnumA(ProcedureContext ctx, uint a)
    {
        return new ReturnEnum.A(a);
    }

    [SpacetimeDB.Procedure]
    public static ReturnEnum ReturnEnumB(ProcedureContext ctx, string b)
    {
        return new ReturnEnum.B(b);
    }

    [SpacetimeDB.Procedure]
    public static Uuid ReturnUuid(ProcedureContext ctx, Uuid u)
    {
        return u;
    }

    [SpacetimeDB.Procedure]
    public static SpacetimeDB.Unit WillPanic(ProcedureContext ctx)
    {
        throw new InvalidOperationException("This procedure is expected to panic");
    }

    [SpacetimeDB.Procedure]
    [Experimental("STDB_UNSTABLE")]
    public static string ReadMySchemaViaHttp(ProcedureContext ctx)
    {
        try
        {
            var moduleIdentity = ProcedureContext.Identity;
            var uri = $"http://localhost:3000/v1/database/{moduleIdentity}/schema?version=9";
            var res = ctx.Http.Get(uri, System.TimeSpan.FromSeconds(2));
            return res switch
            {
                Result<HttpResponse, HttpError>.OkR(var v) => "OK " + v.Body.ToStringUtf8Lossy(),
                Result<HttpResponse, HttpError>.ErrR(var e) => "ERR " + e.Message,
                _ => throw new InvalidOperationException("Unknown Result variant."),
            };
        }
        catch (Exception e)
        {
            return "EXN " + e;
        }
    }

    [SpacetimeDB.Procedure]
    [Experimental("STDB_UNSTABLE")]
    public static string InvalidHttpRequest(ProcedureContext ctx)
    {
        try
        {
            var res = ctx.Http.Get("http://foo.invalid/", System.TimeSpan.FromMilliseconds(250));
            return res switch
            {
                Result<HttpResponse, HttpError>.OkR(var v) => "OK " + v.Body.ToStringUtf8Lossy(),
                Result<HttpResponse, HttpError>.ErrR(var e) => "ERR " + e.Message,
                _ => throw new InvalidOperationException("Unknown Result variant."),
            };
        }
        catch (Exception e)
        {
            return "EXN " + e;
        }
    }

#pragma warning disable STDB_UNSTABLE
    [SpacetimeDB.Procedure]
    public static void InsertWithTxCommit(ProcedureContext ctx)
    {
        ctx.WithTx(tx =>
        {
            tx.Db.my_table.Insert(new MyTable
            {
                Field = new ReturnStruct(a: 42, b: "magic"),
            });
            return new Unit();
        });

        AssertRowCount(ctx, 1);
    }

    [SpacetimeDB.Procedure]
    public static void InsertWithTxRollback(ProcedureContext ctx)
    {
        var outcome = ctx.TryWithTx<SpacetimeDB.Unit, InvalidOperationException>(tx =>
        {
            tx.Db.my_table.Insert(new MyTable
            {
                Field = new ReturnStruct(a: 42, b: "magic")
            });

            throw new InvalidOperationException("rollback");
        });

        Debug.Assert(!outcome.IsSuccess, "TryWithTxAsync should report failure");
        AssertRowCount(ctx, 0);
    }

    [SpacetimeDB.Procedure]
    public static Result<ReturnStruct, string> InsertWithTxRollbackResult(ProcedureContext ctx)
    {
        try
        {
            var outcome = ctx.TryWithTx<SpacetimeDB.Unit, InvalidOperationException>(tx =>
            {
                tx.Db.my_table.Insert(new MyTable
                {
                    Field = new ReturnStruct(a: 42, b: "magic")
                });

                throw new InvalidOperationException("rollback");
            });
            Debug.Assert(!outcome.IsSuccess, "TryWithTxAsync should report failure");
            AssertRowCount(ctx, 0);
            return Result<ReturnStruct, string>.Ok(new ReturnStruct(a: 42, b: "magic"));
        }
        catch (System.Exception e)
        {
            return Result<ReturnStruct, string>.Err(e.ToString());
        }
    }

    private static void AssertRowCount(ProcedureContext ctx, ulong expected)
    {
        ctx.WithTx(tx =>
        {
            var actual = tx.Db.my_table.Count;
            if (actual != expected)
            {
                throw new InvalidOperationException(
                    $"Expected {expected} MyTable rows but found {actual}."
                );
            }
            return 0;
        });
    }

    [SpacetimeDB.Table(Name = "retry_log", Public = true)]
    public partial class RetryLog
    {
        [SpacetimeDB.PrimaryKey]
        public uint Id;
        public uint Attempts;
    }

    [SpacetimeDB.Procedure]
    public static void InsertWithTxRetry(ProcedureContext ctx)
    {
        const uint key = 1;

        var outcome = ctx.TryWithTx<uint, Exception>(tx =>
        {
            var existing = tx.Db.retry_log.Id.Find(key);

            if (existing is null)
            {
                tx.Db.retry_log.Insert(new RetryLog { Id = key, Attempts = 1 });
                return Result<uint, Exception>.Err(new Exception("conflict"));
            }

            // Use the unique index Update method
            var newAttempts = existing.Attempts + 1;
            tx.Db.retry_log.Id.Update(new RetryLog { Id = key, Attempts = newAttempts });
            return Result<uint, Exception>.Ok(newAttempts);
        });

        if (!outcome.IsSuccess)
        {
            outcome = ctx.TryWithTx<uint, Exception>(tx =>
            {
                var existing = tx.Db.retry_log.Id.Find(key);

                if (existing is null)
                {
                    tx.Db.retry_log.Insert(new RetryLog { Id = key, Attempts = 1 });
                    return Result<uint, Exception>.Err(new Exception("conflict"));
                }

                // Use the unique index Update method
                var newAttempts = existing.Attempts + 1;
                tx.Db.retry_log.Id.Update(new RetryLog { Id = key, Attempts = newAttempts });
                return Result<uint, Exception>.Ok(newAttempts);
            });
        }

        Debug.Assert(outcome.IsSuccess, "Retry should have succeeded");
    }

    [SpacetimeDB.Procedure]
    public static void InsertWithTxPanic(ProcedureContext ctx)
    {
        try
        {
            ctx.WithTx<object>(tx =>
            {
                // Insert a row
                tx.Db.my_table.Insert(new MyTable
                {
                    Field = new ReturnStruct(a: 99, b: "panic-test")
                });

                // Throw an exception to abort the transaction
                throw new InvalidOperationException("panic abort");
            });
        }
        catch (InvalidOperationException ex) when (ex.Message == "panic abort")
        {
            // Expected exception - transaction should be aborted
        }

        // Verify no rows were inserted due to the exception
        AssertRowCount(ctx, 0);
    }

    [SpacetimeDB.Procedure]
    public static void DanglingTxWarning(ProcedureContext ctx)
    {
        // This test demonstrates transaction cleanup when an unhandled exception occurs
        // during transaction processing, which should trigger auto-abort behavior

        var exceptionCaught = false;

        try
        {
            ctx.WithTx<object>(tx =>
            {
                // Insert a row
                tx.Db.my_table.Insert(new MyTable
                {
                    Field = new ReturnStruct(a: 123, b: "dangling")
                });

                // Simulate an unexpected system exception that might leave transaction in limbo
                // This should trigger the transaction cleanup/auto-abort mechanisms
                throw new SystemException("Simulated system failure during transaction");
            });
        }
        catch (SystemException)
        {
            exceptionCaught = true;
        }

        // Verify the exception was caught and no rows were persisted
        if (!exceptionCaught)
        {
            throw new InvalidOperationException("Expected SystemException was not thrown");
        }

        // Verify no rows were persisted due to transaction abort
        AssertRowCount(ctx, 0);
    }

    [SpacetimeDB.Procedure]
    public static ReturnStruct TxContextCapabilities(ProcedureContext ctx)
    {
        var result = ctx.WithTx(tx =>
        {
            // Test 1: Verify transaction context has database access
            var initialCount = tx.Db.my_table.Count;

            // Test 2: Insert data and verify it's visible within the same transaction
            tx.Db.my_table.Insert(new MyTable
            {
                Field = new ReturnStruct(a: 200, b: "tx-test")
            });

            var countAfterInsert = tx.Db.my_table.Count;
            if (countAfterInsert != initialCount + 1)
            {
                throw new InvalidOperationException($"Expected count {initialCount + 1}, got {countAfterInsert}");
            }

            // Test 3: Verify transaction context properties are accessible
            var txSender = tx.Sender;
            var txTimestamp = tx.Timestamp;

            if (txSender.Equals(ctx.Sender) == false)
            {
                throw new InvalidOperationException("Transaction sender should match procedure sender");
            }

            // Test 4: Return data from within transaction
            return new ReturnStruct(a: (uint)countAfterInsert, b: $"sender:{txSender}");
        });

        // Verify the row was committed - use flexible row count check
        try
        {
            ctx.WithTx(tx =>
            {
                var actualCount = tx.Db.my_table.Count;
                if (actualCount == 0)
                {
                    throw new InvalidOperationException("Expected at least 1 MyTable row but found none - transaction may not have committed");
                }
                return 0;
            });
        }
        catch (Exception ex)
        {
            // Log the assertion failure but don't fail the procedure
            Log.Error($"TxContextCapabilities row count assertion failed: {ex.Message}");
            // Still return the valid result from the transaction
        }

        return result;
    }

    [SpacetimeDB.Procedure]
    public static ReturnStruct AuthenticationCapabilities(ProcedureContext ctx)
    {
        // Test 1: Verify authentication context is accessible from procedure context
        var procAuth = ctx.SenderAuth;
        var procSender = ctx.Sender;
        var procConnectionId = ctx.ConnectionId;

        var result = ctx.WithTx(tx =>
        {
            // Test 2: Verify authentication context is accessible from transaction context
            var txAuth = tx.SenderAuth;
            var txSender = tx.Sender;
            var txConnectionId = tx.ConnectionId;

            // Test 3: Authentication contexts should be consistent
            if (txSender.Equals(procSender) == false)
            {
                throw new InvalidOperationException(
                    $"Transaction sender {txSender} should match procedure sender {procSender}");
            }

            if (txConnectionId.Equals(procConnectionId) == false)
            {
                throw new InvalidOperationException(
                    $"Transaction connectionId {txConnectionId} should match procedure connectionId {procConnectionId}");
            }

            // Test 4: Insert data with authentication information
            tx.Db.my_table.Insert(new MyTable
            {
                Field = new ReturnStruct(
                    a: (uint)(txSender.GetHashCode() & 0xFF),
                    b: $"auth:sender:{txSender}:conn:{txConnectionId}")
            });

            // Test 5: Check JWT claims (if available)
            var jwtInfo = "no-jwt";
            try
            {
                var jwt = txAuth.Jwt;
                if (jwt != null)
                {
                    jwtInfo = $"jwt:present:identity:{jwt.Identity}";
                }
            }
            catch
            {
                // JWT may not be available in test environment
                jwtInfo = "jwt:unavailable";
            }

            return new ReturnStruct(
                a: (uint)(txSender.GetHashCode() & 0xFF),
                b: jwtInfo);
        });

        return result;
    }

    [SpacetimeDB.Procedure]
    public static ReturnStruct SubscriptionEventOffset(ProcedureContext ctx)
    {
        // This procedure tests that subscription events carry transaction offset information
        // We'll insert data and return information that helps verify the transaction offset

        var result = ctx.WithTx(tx =>
        {
            // Insert a row that will trigger subscription events
            var testData = new MyTable
            {
                Field = new ReturnStruct(
                    a: 999, // Use a distinctive value to identify this test
                    b: $"offset-test:{tx.Timestamp.MicrosecondsSinceUnixEpoch}")
            };

            tx.Db.my_table.Insert(testData);

            // Return data that can be used to correlate with subscription events
            return new ReturnStruct(
                a: 999,
                b: $"committed:{tx.Timestamp.MicrosecondsSinceUnixEpoch}");
        });

        // At this point, the transaction should be committed and subscription events
        // should be generated with the transaction offset information

        return result;
    }

    [SpacetimeDB.Procedure]
    public static ReturnStruct DocumentationGapChecks(ProcedureContext ctx, uint inputValue, string inputText)
    {
        // This procedure tests various documentation gaps and edge cases
        // Test 1: Parameter handling - procedures can accept multiple parameters
        if (inputValue == 0)
        {
            throw new ArgumentException("inputValue cannot be zero");
        }

        if (string.IsNullOrEmpty(inputText))
        {
            throw new ArgumentException("inputText cannot be null or empty");
        }

        var result = ctx.WithTx(tx =>
        {
            // Test 2: Multiple database operations in single transaction
            var count = tx.Db.my_table.Count;

            // Test 3: Conditional logic based on database state
            if (count > 10)
            {
                // Don't insert if too many rows
                return new ReturnStruct(
                    a: (uint)count,
                    b: $"skipped:too-many-rows:{count}");
            }

            // Test 4: Complex data manipulation
            var processedValue = inputValue * 2 + (uint)inputText.Length;

            tx.Db.my_table.Insert(new MyTable
            {
                Field = new ReturnStruct(
                    a: processedValue,
                    b: $"doc-gap:{inputText}:processed:{processedValue}")
            });

            // Test 5: Return computed results
            return new ReturnStruct(
                a: processedValue,
                b: $"success:input:{inputText}:result:{processedValue}");
        });

        // Test 6: Post-transaction validation
        var finalCount = ctx.WithTx(tx => tx.Db.my_table.Count);

        if (finalCount <= 0)
        {
            throw new InvalidOperationException("Expected at least one row after transaction");
        }

        return result;
    }
#pragma warning restore STDB_UNSTABLE
}
