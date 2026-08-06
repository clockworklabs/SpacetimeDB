using System;
using System.Collections.Generic;
using SpacetimeDB;
using SpacetimeDB.Types;

const string DbNameEnvVar = "SPACETIME_SDK_TEST_DB_NAME";
const string ServerUrlEnvVar = "SPACETIME_SDK_TEST_SERVER_URL";

AppDomain.CurrentDomain.UnhandledException += (_, eventArgs) =>
{
    Console.Error.WriteLine(eventArgs.ExceptionObject);
    Environment.Exit(1);
};

var testName = args.Length > 0 ? args[0] : throw new ArgumentException("Pass a test name as argv[1]");
var dbName = Environment.GetEnvironmentVariable(DbNameEnvVar) ?? throw new InvalidOperationException($"{DbNameEnvVar} is not set");
var serverUrl = Environment.GetEnvironmentVariable(ServerUrlEnvVar) ?? "http://localhost:3000";

switch (testName)
{
    case "procedure-return-values":
        RunProcedureReturnValues();
        break;
    case "procedure-observe-panic":
        RunProcedureObservePanic();
        break;
    case "insert-with-tx-commit":
        RunInsertWithTxCommit();
        break;
    case "insert-with-tx-rollback":
        RunInsertWithTxRollback();
        break;
    case "procedure-http-ok":
        RunProcedureHttpOk();
        break;
    case "procedure-http-err":
        RunProcedureHttpErr();
        break;
    case "schedule-procedure":
        RunScheduleProcedure();
        break;
    default:
        throw new ArgumentException($"Unknown C# procedure harness test: {testName}");
}

void RunProcedureReturnValues()
{
    using var test = Connect();
    var remaining = 4;
    void Seen() => remaining--;

    test.Db.Procedures.ReturnPrimitive(1, 2, (_, result) =>
    {
        RequireSuccess(result);
        Require(result.Value == 3, $"Expected return_primitive to return 3, got {result.Value}");
        Seen();
    });
    test.Db.Procedures.ReturnStruct(1234, "foo", (_, result) =>
    {
        RequireSuccess(result);
        Require(result.Value.A == 1234 && result.Value.B == "foo", "Unexpected return_struct result");
        Seen();
    });
    test.Db.Procedures.ReturnEnumA(1234, (_, result) =>
    {
        RequireSuccess(result);
        Require(result.Value is ReturnEnum.A a && a.A_ == 1234, "Unexpected return_enum_a result");
        Seen();
    });
    test.Db.Procedures.ReturnEnumB("foo", (_, result) =>
    {
        RequireSuccess(result);
        Require(result.Value is ReturnEnum.B b && b.B_ == "foo", "Unexpected return_enum_b result");
        Seen();
    });

    test.FrameTickUntil(() => remaining == 0);
}

void RunProcedureObservePanic()
{
    using var test = Connect();
    var observed = false;

    test.Db.Procedures.WillPanic((_, result) =>
    {
        Require(!result.IsSuccess, "Expected will_panic to fail");
        observed = true;
    });

    test.FrameTickUntil(() => observed);
}

void RunInsertWithTxCommit()
{
    using var test = ConnectAndSubscribeAll();
    Require(test.Db.Db.MyTable.Count == 0, "Expected my_table to start empty");

    var inserted = false;
    var callback = false;
    test.Db.Db.MyTable.OnInsert += (_, row) =>
    {
        RequireExpectedReturnStruct(row.Field);
        inserted = true;
    };

    test.Db.Procedures.InsertWithTxCommit((_, result) =>
    {
        RequireSuccess(result);
        Require(test.Db.Db.MyTable.Count == 1, $"Expected one my_table row, got {test.Db.Db.MyTable.Count}");
        RequireExpectedReturnStruct(test.Db.Db.MyTable.Iter().First().Field);
        callback = true;
    });

    test.FrameTickUntil(() => inserted && callback);
}

void RunInsertWithTxRollback()
{
    using var test = ConnectAndSubscribeAll();
    Require(test.Db.Db.MyTable.Count == 0, "Expected my_table to start empty");

    test.Db.Db.MyTable.OnInsert += (_, _) => throw new Exception("Rollback procedure unexpectedly inserted a row");

    var callback = false;
    test.Db.Procedures.InsertWithTxRollback((_, result) =>
    {
        RequireSuccess(result);
        Require(test.Db.Db.MyTable.Count == 0, $"Expected no my_table rows, got {test.Db.Db.MyTable.Count}");
        callback = true;
    });

    test.FrameTickUntil(() => callback);
}

void RunProcedureHttpOk()
{
    using var test = Connect();
    var observed = false;

    test.Db.Procedures.ReadMySchema(serverUrl, (_, result) =>
    {
        RequireSuccess(result);
        Require(result.Value.Contains("\"read_my_schema\""), "Schema response did not include read_my_schema");
        observed = true;
    });

    test.FrameTickUntil(() => observed);
}

void RunProcedureHttpErr()
{
    using var test = Connect();
    var observed = false;

    test.Db.Procedures.InvalidRequest((_, result) =>
    {
        RequireSuccess(result);
        Require(result.Value.Contains("error"), "Expected invalid_request result to mention an error");
        Require(result.Value.Contains("http://foo.invalid/"), "Expected invalid_request result to mention the URL");
        observed = true;
    });

    test.FrameTickUntil(() => observed);
}

void RunScheduleProcedure()
{
    using var test = ConnectAndSubscribeAll();
    Require(test.Db.Db.ProcInsertsInto.Count == 0, "Expected proc_inserts_into to start empty");

    var inserted = false;
    test.Db.Db.ProcInsertsInto.OnInsert += (_, row) =>
    {
        Require(row.X == 42, $"Expected X=42, got {row.X}");
        Require(row.Y == 24, $"Expected Y=24, got {row.Y}");
        var elapsed = row.ProcedureTs.TimeDurationSince(row.ReducerTs);
        Require(elapsed.Microseconds >= 1_000_000, $"Procedure ran too soon: {elapsed.Microseconds}us");
        Require(elapsed.Microseconds <= 2_000_000, $"Procedure ran too late: {elapsed.Microseconds}us");
        inserted = true;
    };

    test.Db.Reducers.ScheduleProc();
    test.FrameTickUntil(() => inserted);
}

void RequireExpectedReturnStruct(ReturnStruct value)
{
    Require(value.A == 42, $"Expected A=42, got {value.A}");
    Require(value.B == "magic", $"Expected B=magic, got {value.B}");
}

TestConnection ConnectAndSubscribeAll()
{
    var test = Connect();
    var applied = false;
    test.Db.SubscriptionBuilder()
        .OnApplied(_ => applied = true)
        .OnError((_, err) => throw err)
        .Subscribe(new[]
        {
            "SELECT * FROM my_table",
            "SELECT * FROM proc_inserts_into",
            "SELECT * FROM pk_uuid",
        });
    test.FrameTickUntil(() => applied);
    return test;
}

TestConnection Connect()
{
    var connected = false;
    var disconnected = false;
    DbConnection? conn = null;

    conn = DbConnection.Builder()
        .WithUri(serverUrl)
        .WithDatabaseName(dbName)
        .OnConnect((_, _, _) => connected = true)
        .OnConnectError(err => throw err)
        .OnDisconnect((_, err) =>
        {
            disconnected = true;
            throw new Exception("Unexpected disconnect", err);
        })
        .Build();

    var test = new TestConnection(conn, () => disconnected);
    test.FrameTickUntil(() => connected);
    return test;
}

void RequireSuccess<T>(ProcedureCallbackResult<T> result)
{
    if (!result.IsSuccess)
    {
        throw new Exception("Procedure failed", result.Error);
    }
}

void Require(bool condition, string message)
{
    if (!condition)
    {
        throw new Exception(message);
    }
}

sealed class TestConnection(DbConnection db, Func<bool> disconnected) : IDisposable
{
    public DbConnection Db { get; } = db;

    public void FrameTickUntil(Func<bool> predicate)
    {
        var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(20);
        while (!predicate())
        {
            if (DateTime.UtcNow >= deadline)
            {
                throw new Exception("Timed out waiting for test condition");
            }
            if (disconnected())
            {
                throw new Exception("Connection disconnected before test completed");
            }
            Db.FrameTick();
            Thread.Sleep(1);
        }
    }

    public void Dispose() => Db.Disconnect();
}
