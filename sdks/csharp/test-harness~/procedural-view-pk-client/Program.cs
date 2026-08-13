using System;
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
    case "sender-scoped-pk-view":
        RunSenderScopedPkView();
        break;
    case "view-pk-left-semijoin":
        RunViewPkLeftSemijoin();
        break;
    case "view-pk-right-semijoin":
        RunViewPkRightSemijoin();
        break;
    default:
        throw new ArgumentException($"Unknown C# procedural-view-pk harness test: {testName}");
}

void RunSenderScopedPkView()
{
    using var senderA = Connect();
    using var senderB = Connect();
    var senderAUpdated = false;
    var senderBUpdated = false;

    SubscribeThen(senderA, builder => builder.AddQuery(qb => qb.From.SenderLeftView()), ctx =>
    {
        ctx.Db.SenderLeftView.OnUpdate += (_, oldRow, newRow) =>
        {
            RequireLeft(oldRow, 1, 10);
            RequireLeft(newRow, 1, 11);
            senderAUpdated = true;
        };
        ctx.Reducers.InsertLeft(1, 10);
        ctx.Reducers.UpdateLeft(1, 11);
    });

    SubscribeThen(senderB, builder => builder.AddQuery(qb => qb.From.SenderLeftView()), ctx =>
    {
        ctx.Db.SenderLeftView.OnUpdate += (_, oldRow, newRow) =>
        {
            RequireLeft(oldRow, 2, 20);
            RequireLeft(newRow, 2, 21);
            senderBUpdated = true;
        };
        ctx.Reducers.InsertLeft(2, 20);
        ctx.Reducers.UpdateLeft(2, 21);
    });

    FrameTickUntil(() => senderAUpdated && senderBUpdated, senderA, senderB);
}

void RunViewPkLeftSemijoin()
{
    using var test = Connect();
    var inserted = false;

    SubscribeThen(test, builder => builder.AddQuery(qb => qb.From.SenderRightView()
        .Filter(right => right.Filter.Eq(300UL))
        .RightSemijoin(qb.From.SenderLeftView(), (right, left) => right.Id.Eq(left.Id))
        .Filter(left => left.Filter.Eq(100UL))), ctx =>
    {
        ctx.Db.SenderLeftView.OnInsert += (eventCtx, row) =>
        {
            Require(eventCtx.Db.SenderLeftView.Count == 1, $"Expected one left view row, got {eventCtx.Db.SenderLeftView.Count}");
            RequireLeft(row, 10, 100);
            inserted = true;
        };
        InsertSemijoinSourceRows(ctx);
    });

    test.FrameTickUntil(() => inserted);
}

void RunViewPkRightSemijoin()
{
    using var test = Connect();
    var inserted = false;

    SubscribeThen(test, builder => builder.AddQuery(qb => qb.From.SenderLeftView()
        .Filter(left => left.Filter.Eq(100UL))
        .RightSemijoin(qb.From.SenderRightView(), (left, right) => left.Id.Eq(right.Id))
        .Filter(right => right.Filter.Eq(300UL))), ctx =>
    {
        ctx.Db.SenderRightView.OnInsert += (eventCtx, row) =>
        {
            Require(eventCtx.Db.SenderRightView.Count == 1, $"Expected one right view row, got {eventCtx.Db.SenderRightView.Count}");
            RequireRight(row, 10, 300);
            inserted = true;
        };
        InsertSemijoinSourceRows(ctx);
    });

    test.FrameTickUntil(() => inserted);
}

void InsertSemijoinSourceRows(SubscriptionEventContext ctx)
{
    ctx.Reducers.InsertLeft(10, 100);
    ctx.Reducers.InsertLeft(20, 200);
    ctx.Reducers.InsertRight(10, 300);
    ctx.Reducers.InsertRight(20, 400);
}

void SubscribeThen(TestConnection test, Func<SubscriptionBuilder, TypedSubscriptionBuilder> build, Action<SubscriptionEventContext> onApplied)
{
    var applied = false;
    build(test.Db.SubscriptionBuilder())
        .OnApplied(ctx =>
        {
            applied = true;
            onApplied(ctx);
        })
        .OnError((_, err) => throw err)
        .Subscribe();
    test.FrameTickUntil(() => applied);
}

void RequireLeft(LeftSource row, ulong id, ulong filter)
{
    Require(row.Id == id, $"Expected left id {id}, got {row.Id}");
    Require(row.Filter == filter, $"Expected left filter {filter}, got {row.Filter}");
}

void RequireRight(RightSource row, ulong id, ulong filter)
{
    Require(row.Id == id, $"Expected right id {id}, got {row.Id}");
    Require(row.Filter == filter, $"Expected right filter {filter}, got {row.Filter}");
}

TestConnection Connect()
{
    var connected = false;
    var disconnected = false;
    var conn = DbConnection.Builder()
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

void FrameTickUntil(Func<bool> predicate, params TestConnection[] connections)
{
    var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(20);
    while (!predicate())
    {
        if (DateTime.UtcNow >= deadline)
        {
            throw new Exception("Timed out waiting for test condition");
        }
        foreach (var connection in connections)
        {
            if (connection.Disconnected())
            {
                throw new Exception("Connection disconnected before test completed");
            }
            connection.Db.FrameTick();
        }
        Thread.Sleep(1);
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
    public bool Disconnected() => disconnected();

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
