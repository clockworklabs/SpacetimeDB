/// Regression tests run with a live server.
/// To run these, run a local SpacetimeDB via `spacetime start`,
/// then in a separate terminal run `tools~/run-regression-tests.sh PATH_TO_SPACETIMEDB_REPO_CHECKOUT`.
/// This is done on CI in .github/workflows/test.yml.
using System;
using System.Diagnostics;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Threading;
using SpacetimeDB;
using SpacetimeDB.Types;

const string HOST = "http://localhost:3000";
const string DBNAME = "btree-repro";

DbConnection ConnectToDB()
{
    DbConnection? conn = null;
    conn = DbConnection
        .Builder()
        .WithUri(HOST)
        .WithModuleName(DBNAME)
        .OnConnect(OnConnected)
        .OnConnectError(
            (err) =>
            {
                throw err;
            }
        )
        .OnDisconnect(
            (conn, err) =>
            {
                if (err != null)
                {
                    throw err;
                }
                else
                {
                    throw new Exception("Unexpected disconnect");
                }
            }
        )
        .Build();
    return conn;
}

uint waiting = 0;
var applied = false;
SubscriptionHandle? handle = null;

void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    Log.Debug($"Connected to {DBNAME} on {HOST}");
    handle = conn.SubscriptionBuilder()
        .OnApplied(OnSubscriptionApplied)
        .OnError(
            (ctx, err) =>
            {
                throw err;
            }
        )
        .Subscribe([
            "SELECT * FROM example_data",
            "SELECT * FROM my_player",
            "SELECT * FROM players_at_level_one",
        ]);

    conn.Reducers.OnAdd += (ReducerEventContext ctx, uint id, uint indexed) =>
    {
        Log.Info("Got Add callback");
        waiting--;
        ValidateBTreeIndexes(ctx);
    };

    conn.Reducers.OnDelete += (ReducerEventContext ctx, uint id) =>
    {
        Log.Info("Got Delete callback");
        waiting--;
        ValidateBTreeIndexes(ctx);
    };

    conn.OnUnhandledReducerError += (ReducerEventContext ctx, Exception exception) =>
    {
        Log.Info($"Got OnUnhandledReducerError: {exception}");
        waiting--;
        ValidateBTreeIndexes(ctx);
    };
}

const uint MAX_ID = 10;

// Test that indexes remain in sync with the expected table state when deletes are received.
// This used to fail, when row types did not correctly implement IEquatable.
void ValidateBTreeIndexes(IRemoteDbContext conn)
{
    Log.Debug("Checking indexes...");
    foreach (var data in conn.Db.ExampleData.Iter())
    {
        Debug.Assert(conn.Db.ExampleData.Indexed.Filter(data.Id).Contains(data));
    }
    var outOfIndex = conn.Db.ExampleData.Iter().ToHashSet();

    for (uint i = 0; i < MAX_ID; i++)
    {
        foreach (var data in conn.Db.ExampleData.Indexed.Filter(i))
        {
            Debug.Assert(outOfIndex.Contains(data));
        }
    }
    Log.Debug("   Indexes are good.");
}

void OnSubscriptionApplied(SubscriptionEventContext context)
{
    applied = true;

    // Do some operations that alter row state;
    // we will check that everything is in sync in the callbacks for these reducer calls.
    Log.Debug("Calling Add");
    waiting++;
    context.Reducers.Add(1, 1);

    Log.Debug("Calling Delete");
    waiting++;
    context.Reducers.Delete(1);

    Log.Debug("Calling Add");
    waiting++;
    context.Reducers.Add(1, 1);

    Log.Debug("Calling ThrowError");
    waiting++;
    context.Reducers.ThrowError("this is an error");

    // RemoteQuery test
    Log.Debug("Calling RemoteQuery");
    var remoteRows = context.Db.ExampleData.RemoteQuery("WHERE Id = 1").Result;
    Debug.Assert(remoteRows != null && remoteRows.Length > 0);

    // Now unsubscribe and check that the unsubscribe is actually applied.
    Log.Debug("Calling Unsubscribe");
    waiting++;
    handle?.UnsubscribeThen(
        (ctx) =>
        {
            Log.Debug("Received Unsubscribe");
            ValidateBTreeIndexes(ctx);
            waiting--;
        }
    );

    // Views test
    Log.Debug("Checking Views are populated");
    Debug.Assert(context.Db.MyPlayer != null, "context.Db.MyPlayer != null");
    Debug.Assert(context.Db.PlayersAtLevelOne != null, "context.Db.PlayersAtLevelOne != null");
    Debug.Assert(
        context.Db.MyPlayer.Count > 0,
        $"context.Db.MyPlayer.Count = {context.Db.MyPlayer.Count}"
    );
    Debug.Assert(
        context.Db.PlayersAtLevelOne.Count > 0,
        $"context.Db.PlayersAtLevelOne.Count = {context.Db.PlayersAtLevelOne.Count}"
    );
    
    Log.Debug("Calling Iter on View");
    var viewIterRows = context.Db.MyPlayer.Iter();
    var expectedPlayer = new Player
    {
        Id = 1,
        Identity = context.Identity!.Value,
        Name = "NewPlayer",
    };
    Log.Debug(
        "MyPlayer Iter count: " + (viewIterRows != null ? viewIterRows.Count().ToString() : "null")
    );
    Debug.Assert(viewIterRows != null && viewIterRows.Any());
    Log.Debug(
        "Validating View row data "
            + $"Id={expectedPlayer.Id}, Identity={expectedPlayer.Identity}, Name={expectedPlayer.Name} => "
            + $"Id={viewIterRows.First().Id}, Identity={viewIterRows.First().Identity}, Name={viewIterRows.First().Name}"
    );
    Debug.Assert(viewIterRows.First().Equals(expectedPlayer));
    
    Log.Debug("Calling RemoteQuery on View");
    var viewRemoteQueryRows = context.Db.MyPlayer.RemoteQuery("WHERE Id > 0");
    Debug.Assert(viewRemoteQueryRows != null && viewRemoteQueryRows.Result.Length > 0);
    Debug.Assert(viewRemoteQueryRows.Result.First().Equals(expectedPlayer));
    
    Log.Debug("Calling Iter on Anonymous View");
    var anonViewIterRows = context.Db.PlayersAtLevelOne.Iter();
    var expectedPlayerAndLevel = new PlayerAndLevel
    {
        Id = 1,
        Identity = context.Identity!.Value,
        Name = "NewPlayer",
        Level = 1,
    };
    Log.Debug(
        "PlayersAtLevelOne Iter count: "
            + (anonViewIterRows != null ? anonViewIterRows.Count().ToString() : "null")
    );
    Debug.Assert(anonViewIterRows != null && anonViewIterRows.Any());
    Log.Debug(
        "Validating Anonymous View row data "
            + $"Id={expectedPlayerAndLevel.Id}, Identity={expectedPlayerAndLevel.Identity}, Name={expectedPlayerAndLevel.Name}, Level={expectedPlayerAndLevel.Level} => "
            + $"Id={anonViewIterRows.First().Id}, Identity={anonViewIterRows.First().Identity}, Name={anonViewIterRows.First().Name}, Level={anonViewIterRows.First().Level} => "    
        //+ $"PlayerId={anonViewIterRows.First().PlayerId}, Level={anonViewIterRows.First().Level}"
    );
    Debug.Assert(anonViewIterRows.First().Equals(expectedPlayerAndLevel));
    
    Log.Debug("Calling RemoteQuery on Anonymous View");
    var anonViewRemoteQueryRows = context.Db.PlayersAtLevelOne.RemoteQuery("WHERE Level = 1");
    Log.Debug(
        "PlayersAtLevelOne RemoteQuery count: "
            + (
                anonViewRemoteQueryRows != null
                    ? anonViewRemoteQueryRows.Result.Length.ToString()
                    : "null"
            )
    );
    Debug.Assert(anonViewRemoteQueryRows != null && anonViewRemoteQueryRows.Result.Length > 0);
    Debug.Assert(anonViewRemoteQueryRows.Result.First().Equals(expectedPlayerAndLevel));
    
    // Procedures tests
    
    Log.Debug("Calling InsertWithTxCommit");
    waiting++;
    context.Procedures.InsertWithTxCommit((IProcedureEventContext ctx, ProcedureCallbackResult<SpacetimeDB.Unit> result) =>
    {
        try
        {
            Debug.Assert(result.IsSuccess, "InsertWithTxCommit should succeed");
            Debug.Assert(context.Db.MyTable.Count == 1, $"MyTable should have one row after commit, but had {context.Db.MyTable.Count}");
        }
        finally
        {
            waiting--;
        }
    });
    
    Log.Debug("Calling InsertWithTxRollback");
    waiting++;
    context.Procedures.InsertWithTxRollback((IProcedureEventContext ctx, ProcedureCallbackResult<SpacetimeDB.Unit> result) =>
    {
        try
        {
            Debug.Assert(!result.IsSuccess, "InsertWithTxRollback should fail");
            Debug.Assert(result.Error is InvalidOperationException ioe && ioe.Message == "rollback", $"Expected error to be InvalidOperationException with message 'rollback' but got {result.Error}");
            Debug.Assert(context.Db.MyTable.Count == 0, "MyTable should remain empty after rollback");
            // No tx-offset assertion because ProcedureEvent doesn’t expose one yet.
        }
        finally
        {
            waiting--;
        }
    });
    
    Log.Debug("Calling InsertWithTxRetry");
    waiting++;
    context.Procedures.InsertWithTxRetry((IProcedureEventContext ctx, ProcedureCallbackResult<uint> result) =>
    {
        try
        {
            Debug.Assert(result.IsSuccess, "InsertWithTxRetry should succeed after retry");
            // For Unit return types, you don't need to check result.Value
        }
        catch (Exception ex)
        {
            Log.Exception(ex);
            throw;
        }
        finally
        {
            waiting--;
        }
    });

    Log.Debug("Calling InsertWithTxPanic");
    waiting++;
    context.Procedures.InsertWithTxPanic((IProcedureEventContext ctx, ProcedureCallbackResult<SpacetimeDB.Unit> result) =>
    {
        try
        {
            Debug.Assert(result.IsSuccess, "InsertWithTxPanic should succeed (exception is caught)");
            Debug.Assert(context.Db.MyTable.Count == 0, "MyTable should remain empty after exception abort");
        }
        finally
        {
            waiting--;
        }
    });

    Log.Debug("Calling DanglingTxWarning");
    waiting++;
    context.Procedures.DanglingTxWarning((IProcedureEventContext ctx, ProcedureCallbackResult<SpacetimeDB.Unit> result) =>
    {
        try
        {
            Debug.Assert(result.IsSuccess, "DanglingTxWarning should succeed");
            Debug.Assert(context.Db.MyTable.Count == 0, "MyTable should remain empty after dangling tx auto-abort");
            // Note: We can't easily assert on the warning log from client-side,
            // but the server-side AssertRowCount verifies the auto-abort behavior
        }
        finally
        {
            waiting--;
        }
    });

    Log.Debug("Calling TxContextCapabilities");
    waiting++;
    context.Procedures.TxContextCapabilities((IProcedureEventContext ctx, ProcedureCallbackResult<ReturnStruct> result) =>
    {
        try
        {
            Debug.Assert(result.IsSuccess, "TxContextCapabilities should succeed");
            Debug.Assert(result.Value != null && result.Value.A == 1, $"Expected count 1, got {result.Value.A}");
            Debug.Assert(result.Value.B.StartsWith("sender:"), $"Expected sender info, got {result.Value.B}");
            Debug.Assert(context.Db.MyTable.Count == 1, "MyTable should have one row after TxContext test");
            
            // Verify the inserted row has the expected data
            var row = context.Db.MyTable.Iter().FirstOrDefault();
            Debug.Assert(row is not null, "Should have a row in MyTable");
            Debug.Assert(row.Field.A == 200, $"Expected field.A == 200, got {row.Field.A}");
            Debug.Assert(row.Field.B == "tx-test", $"Expected field.B == 'tx-test', got {row.Field.B}");
        }
        finally
        {
            waiting--;
        }
    });

    Log.Debug("Calling TimestampCapabilities");
    waiting++;
    context.Procedures.TimestampCapabilities((IProcedureEventContext ctx, ProcedureCallbackResult<ReturnStruct> result) =>
    {
        try
        {
            Debug.Assert(result.IsSuccess, "TimestampCapabilities should succeed");
            Debug.Assert(result.Value != null && result.Value.A > 0, "Should return a valid timestamp-derived value");
            Debug.Assert(result.Value.B.Contains(":"), "Should return formatted timestamp string");
            Debug.Assert(context.Db.MyTable.Count == 2, "MyTable should have two rows after timestamp test");
            
            // Verify the inserted row has timestamp information
            var rows = context.Db.MyTable.Iter().ToList();
            Debug.Assert(rows.Count == 2, "Should have exactly 2 rows");
            
            var timestampRow = rows.FirstOrDefault(r => r.Field.B.StartsWith("timestamp:"));
            Debug.Assert(timestampRow is not null, "Should have a row with timestamp data");
            Debug.Assert(timestampRow.Field.B.StartsWith("timestamp:"), "Timestamp row should have correct format");
        }
        finally
        {
            waiting--;
        }
    });

    Log.Debug("Calling AuthenticationCapabilities");
    waiting++;
    context.Procedures.AuthenticationCapabilities((IProcedureEventContext ctx, ProcedureCallbackResult<ReturnStruct> result) =>
    {
        try
        {
            Debug.Assert(result.IsSuccess, "AuthenticationCapabilities should succeed");
            Debug.Assert(result.Value != null, "Should return a valid sender-derived value");
            Debug.Assert(result.Value.B.Contains("jwt:") || result.Value.B == "no-jwt", $"Should return JWT info, got {result.Value.B}");
            Debug.Assert(context.Db.MyTable.Count == 3, "MyTable should have three rows after auth test");
            
            // Verify the inserted row has authentication information
            var rows = context.Db.MyTable.Iter().ToList();
            Debug.Assert(rows.Count == 3, "Should have exactly 3 rows");
            
            var authRow = rows.FirstOrDefault(r => r.Field.B.StartsWith("auth:"));
            Debug.Assert(authRow is not null, "Should have a row with auth data");
            Debug.Assert(authRow.Field.B.Contains("sender:"), "Auth row should contain sender info");
            Debug.Assert(authRow.Field.B.Contains("conn:"), "Auth row should contain connection info");
        }
        finally
        {
            waiting--;
        }
    });

    Log.Debug("Calling SubscriptionEventOffset");
    waiting++;
    context.Procedures.SubscriptionEventOffset((IProcedureEventContext ctx, ProcedureCallbackResult<ReturnStruct> result) =>
    {
        try
        {
            Debug.Assert(result.IsSuccess, "SubscriptionEventOffset should succeed");
            Debug.Assert(result.Value != null && result.Value.A == 999, $"Expected A == 999, got {result.Value.A}");
            Debug.Assert(result.Value.B.StartsWith("committed:"), $"Expected committed timestamp, got {result.Value.B}");
            Debug.Assert(context.Db.MyTable.Count == 4, "MyTable should have four rows after offset test");
            
            // Verify the inserted row has the expected offset test data
            var rows = context.Db.MyTable.Iter().ToList();
            Debug.Assert(rows.Count == 4, "Should have exactly 4 rows");
            
            var offsetRow = rows.FirstOrDefault(r => r.Field.B.StartsWith("offset-test:"));
            Debug.Assert(offsetRow is not null, "Should have a row with offset-test data");
            Debug.Assert(offsetRow.Field.A == 999, "Offset test row should have A == 999");
            
            // Note: Transaction offset information may not be directly accessible in ProcedureEvent yet,
            // but this test verifies that the transaction was committed and subscription events were generated
            // The presence of the new row in the subscription confirms the transaction offset was processed
        }
        finally
        {
            waiting--;
        }
    });

    Log.Debug("Calling DocumentationGapChecks with valid parameters");
    waiting++;
    context.Procedures.DocumentationGapChecks(42, "test-input", (IProcedureEventContext ctx, ProcedureCallbackResult<ReturnStruct> result) =>
    {
        try
        {
            Debug.Assert(result.IsSuccess, "DocumentationGapChecks should succeed with valid parameters");
            
            // Expected: inputValue * 2 + inputText.Length = 42 * 2 + 10 = 94
            var expectedValue = 42u * 2 + (uint)"test-input".Length; // 84 + 10 = 94
            Debug.Assert(result.Value != null && result.Value.A == expectedValue, $"Expected A == {expectedValue}, got {result.Value.A}");
            Debug.Assert(result.Value.B.StartsWith("success:"), $"Expected success message, got {result.Value.B}");
            Debug.Assert(result.Value.B.Contains("test-input"), "Result should contain input text");
            
            Debug.Assert(context.Db.MyTable.Count == 5, "MyTable should have five rows after documentation gap test");
            
            // Verify the inserted row has the expected documentation gap test data
            var rows = context.Db.MyTable.Iter().ToList();
            var docGapRow = rows.FirstOrDefault(r => r.Field.B.StartsWith("doc-gap:"));
            Debug.Assert(docGapRow is not null, "Should have a row with doc-gap data");
            Debug.Assert(docGapRow.Field.A == expectedValue, $"Doc gap row should have A == {expectedValue}");
            Debug.Assert(docGapRow.Field.B.Contains("test-input"), "Doc gap row should contain input text");
        }
        finally
        {
            waiting--;
        }
    });

    // Test error handling with invalid parameters
    Log.Debug("Calling DocumentationGapChecks with invalid parameters (should fail)");
    waiting++;
    context.Procedures.DocumentationGapChecks(0, "", (IProcedureEventContext ctx, ProcedureCallbackResult<ReturnStruct> result) =>
    {
        try
        {
            Debug.Assert(!result.IsSuccess, "DocumentationGapChecks should fail with invalid parameters");
            Debug.Assert(result.Error is ArgumentException, $"Expected ArgumentException, got {result.Error?.GetType()}");
            
            // Table count should remain the same since the procedure failed
            Debug.Assert(context.Db.MyTable.Count == 5, "MyTable count should remain 5 after failed call");
        }
        finally
        {
            waiting--;
        }
    });
}

System.AppDomain.CurrentDomain.UnhandledException += (sender, args) =>
{
    Log.Exception($"Unhandled exception: {sender} {args}");
    Environment.Exit(1);
};
var db = ConnectToDB();
Log.Info("Starting timer");
const int TIMEOUT = 20; // seconds;
var start = DateTime.Now;
while (!applied || waiting > 0)
{
    db.FrameTick();
    Thread.Sleep(100);
    if ((DateTime.Now - start).Seconds > TIMEOUT)
    {
        Log.Error($"Timeout, all events should have elapsed in {TIMEOUT} seconds!");
        Environment.Exit(1);
    }
}
Log.Info("Success");
Environment.Exit(0);
