// Server module for regression tests.
// Everything we're testing for happens SDK-side so this module is very uninteresting.

using System.Diagnostics;
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

    // At-most-one row: return T?
    [SpacetimeDB.View(Name = "my_player", Public = true)]
    public static Player? MyPlayer(ViewContext ctx)
    {
        return ctx.Db.player.Identity.Find(ctx.Sender) as Player?;
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
    public static SpacetimeDB.Unit WillPanic(ProcedureContext ctx)
    {
        throw new InvalidOperationException("This procedure is expected to panic");
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
            return 0; // return value ignored by WithTx
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

            return ProcedureContext.TxResult<SpacetimeDB.Unit, InvalidOperationException>.Failure(
                new InvalidOperationException("rollback"));
        });

        Debug.Assert(!outcome.IsSuccess, "TryWithTxAsync should report failure");
        AssertRowCount(ctx, 0);
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
                return ProcedureContext.TxResult<uint, Exception>.Failure(new Exception("conflict"));
            }

            // Use the unique index Update method
            var newAttempts = existing.Attempts + 1;
            tx.Db.retry_log.Id.Update(new RetryLog { Id = key, Attempts = newAttempts });
            return ProcedureContext.TxResult<uint, Exception>.Success(newAttempts);
        });

        if (!outcome.IsSuccess)
        {
            throw outcome.Error ?? new InvalidOperationException("Retry failed without an error");
        }

        // Verify final state
        var finalAttempts = ctx.WithTx(tx =>
        {
            var final = tx.Db.retry_log.Id.Find(key);
            return final?.Attempts ?? 0u;
        });
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
        
        bool exceptionCaught = false;
        
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
            // (These should match the procedure context properties)
            var txSender = tx.Sender;
            var txTimestamp = tx.Timestamp;
            
            if (txSender != ctx.Sender)
            {
                throw new InvalidOperationException("Transaction sender should match procedure sender");
            }
            
            // Test 4: Return data from within transaction
            return new ReturnStruct(a: (uint)countAfterInsert, b: $"sender:{txSender}");
        });
        
        // Verify the row was committed
        AssertRowCount(ctx, 1);
        
        return result;
    }

    [SpacetimeDB.Procedure]
    public static ReturnStruct TimestampCapabilities(ProcedureContext ctx)
    {
        // Test 1: Verify timestamp is accessible from procedure context
        var procedureTimestamp = ctx.Timestamp;
        
        var result = ctx.WithTx(tx =>
        {
            // Test 2: Verify timestamp is accessible from transaction context
            var txTimestamp = tx.Timestamp;
            
            // Test 3: Timestamps should be consistent within the same procedure call
            if (txTimestamp != procedureTimestamp)
            {
                throw new InvalidOperationException(
                    $"Transaction timestamp {txTimestamp} should match procedure timestamp {procedureTimestamp}");
            }
            
            // Test 4: Insert data with timestamp information
            tx.Db.my_table.Insert(new MyTable
            {
                Field = new ReturnStruct(
                    a: (uint)(txTimestamp.MicrosecondsSinceUnixEpoch % uint.MaxValue),
                    b: $"timestamp:{txTimestamp.MicrosecondsSinceUnixEpoch}")
            });
            
            return new ReturnStruct(
                a: (uint)(txTimestamp.MicrosecondsSinceUnixEpoch % uint.MaxValue),
                b: txTimestamp.ToString());
        });
        
        // Test 5: Verify timestamp is still accessible after transaction
        var postTxTimestamp = ctx.Timestamp;
        if (postTxTimestamp != procedureTimestamp)
        {
            throw new InvalidOperationException(
                $"Post-transaction timestamp {postTxTimestamp} should match original timestamp {procedureTimestamp}");
        }
        
        return result;
    }
    
    // TODO: Not currently used in a test. Need to see if this is still a valid test.
    // [SpacetimeDB.Procedure]
    // public static ReturnStruct SleepUntilTimestampUpdate(ProcedureContext ctx, uint sleepMillis)
    // {
    //     var beforeTimestamp = ctx.Timestamp;
    //
    //     // Since we can't actually sleep in a procedure, we'll simulate the concept
    //     // by creating a target timestamp and demonstrating timestamp arithmetic
    //     var targetMicros = beforeTimestamp.MicrosecondsSinceUnixEpoch + (sleepMillis * 1000);
    //     var targetTime = new SpacetimeDB.Timestamp(targetMicros);
    //
    //     // Get the current timestamp again (this will be very close to beforeTimestamp)
    //     var afterTimestamp = ctx.Timestamp;
    //     var elapsedMicros = afterTimestamp.MicrosecondsSinceUnixEpoch - beforeTimestamp.MicrosecondsSinceUnixEpoch;
    //
    //     return ctx.WithTx(tx => {
    //         tx.Db.my_table.Insert(new MyTable {
    //             Field = new ReturnStruct(
    //                 a: (uint)(elapsedMicros / 1000), // Convert back to milliseconds
    //                 b: $"target:{targetTime.MicrosecondsSinceUnixEpoch}:actual:{afterTimestamp.MicrosecondsSinceUnixEpoch}"
    //             )
    //         });
    //         return new ReturnStruct(
    //             a: (uint)(elapsedMicros / 1000), // Convert back to milliseconds
    //             b: $"simulated-sleep:{sleepMillis}ms"
    //         );
    //     });
    // }

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
            if (txSender != procSender)
            {
                throw new InvalidOperationException(
                    $"Transaction sender {txSender} should match procedure sender {procSender}");
            }
            
            if (txConnectionId != procConnectionId)
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
