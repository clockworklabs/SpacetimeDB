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
    case "view-pk-on-update":
        RunViewPkOnUpdate();
        break;
    case "view-pk-join-query-builder":
        RunViewPkJoinQueryBuilder();
        break;
    case "view-pk-semijoin-two-sender-views-query-builder":
        RunViewPkSemijoinTwoSenderViewsQueryBuilder();
        break;
    default:
        throw new ArgumentException($"Unknown C# view-pk harness test: {testName}");
}

void RunViewPkOnUpdate()
{
    using var test = Connect();
    var updated = false;

    SubscribeThen(test, builder => builder.AddQuery(qb => qb.From.AllViewPkPlayers()), ctx =>
    {
        ctx.Db.AllViewPkPlayers.OnUpdate += (_, oldRow, newRow) =>
        {
            RequirePlayer(oldRow, 1, "before");
            RequirePlayer(newRow, 1, "after");
            updated = true;
        };

        ctx.Reducers.InsertViewPkPlayer(1, "before");
        ctx.Reducers.UpdateViewPkPlayer(1, "after");
    });

    test.FrameTickUntil(() => updated);
}

void RunViewPkJoinQueryBuilder()
{
    using var test = Connect();
    var updated = false;

    SubscribeThen(test, builder => builder.AddQuery(qb => qb.From.ViewPkMembership()
        .RightSemijoin(qb.From.AllViewPkPlayers(), (membership, player) => membership.PlayerId.Eq(player.Id))), ctx =>
    {
        ctx.Db.AllViewPkPlayers.OnUpdate += (_, oldRow, newRow) =>
        {
            RequirePlayer(oldRow, 1, "before");
            RequirePlayer(newRow, 1, "after");
            updated = true;
        };

        ctx.Reducers.InsertViewPkPlayer(1, "before");
        ctx.Reducers.InsertViewPkMembership(1, 1);
        ctx.Reducers.UpdateViewPkPlayer(1, "after");
    });

    test.FrameTickUntil(() => updated);
}

void RunViewPkSemijoinTwoSenderViewsQueryBuilder()
{
    using var test = Connect();
    var updated = false;

    SubscribeThen(test, builder => builder.AddQuery(qb => qb.From.SenderViewPkPlayersA()
        .RightSemijoin(qb.From.SenderViewPkPlayersB(), (left, right) => left.Id.Eq(right.Id))), ctx =>
    {
        ctx.Db.SenderViewPkPlayersB.OnUpdate += (_, oldRow, newRow) =>
        {
            RequirePlayer(oldRow, 1, "before");
            RequirePlayer(newRow, 1, "after");
            updated = true;
        };

        ctx.Reducers.InsertViewPkPlayer(1, "before");
        ctx.Reducers.InsertViewPkMembership(1, 1);
        ctx.Reducers.InsertViewPkMembershipSecondary(1, 1);
        ctx.Reducers.UpdateViewPkPlayer(1, "after");
    });

    test.FrameTickUntil(() => updated);
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

void RequirePlayer(ViewPkPlayer row, ulong id, string name)
{
    Require(row.Id == id, $"Expected player id {id}, got {row.Id}");
    Require(row.Name == name, $"Expected player name {name}, got {row.Name}");
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
