using System;
using System.Linq;
using System.Threading;
using SpacetimeDB;
using SpacetimeDB.Types;

const string DbNameEnvVar = "SPACETIME_SDK_TEST_DB_NAME";
const string ServerUrlEnvVar = "SPACETIME_SDK_TEST_SERVER_URL";

AppDomain.CurrentDomain.UnhandledException += (_, eventArgs) =>
{
    Console.Error.WriteLine(eventArgs.ExceptionObject);
    Environment.Exit(1);
};

var dbName = Environment.GetEnvironmentVariable(DbNameEnvVar) ?? throw new InvalidOperationException($"{DbNameEnvVar} is not set");
var serverUrl = Environment.GetEnvironmentVariable(ServerUrlEnvVar) ?? "http://localhost:3000";

DbConnection db = null!;
var connected = false;
var connectedRowSeen = false;
var disconnected = false;
Identity? firstIdentity = null;

db = DbConnection
    .Builder()
    .WithUri(serverUrl)
    .WithDatabaseName(dbName)
    .OnConnect((conn, identity, _) =>
    {
        if (identity != conn.Identity)
        {
            throw new Exception("Connection identity callback did not match connection state");
        }
        firstIdentity = identity;
        conn.SubscriptionBuilder()
            .OnApplied(_ =>
            {
                if (conn.Db.Connected.Count != 1)
                {
                    throw new Exception($"Expected one connected row, got {conn.Db.Connected.Count}");
                }

                var row = conn.Db.Connected.Iter().Single();
                if (row.Identity != firstIdentity)
                {
                    throw new Exception("Connected row identity did not match first connection identity");
                }

                connectedRowSeen = true;
                conn.Disconnect();
            })
            .OnError((_, err) => throw err)
            .AddQuery(qb => qb.From.Connected())
            .Subscribe();
        connected = true;
    })
    .OnConnectError(err => throw err)
    .OnDisconnect((_, err) =>
    {
        if (err != null)
        {
            throw err;
        }
        disconnected = true;
    })
    .Build();

FrameTickUntil(() => connected && connectedRowSeen && disconnected);
db.Disconnect();

DbConnection reconnectDb = null!;
var reconnected = false;
var disconnectedRowSeen = false;

reconnectDb = DbConnection
    .Builder()
    .WithUri(serverUrl)
    .WithDatabaseName(dbName)
    .OnConnect((conn, _, _) =>
    {
        conn.SubscriptionBuilder()
            .OnApplied(_ =>
            {
                if (conn.Db.Disconnected.Count != 1)
                {
                    throw new Exception($"Expected one disconnected row, got {conn.Db.Disconnected.Count}");
                }

                var row = conn.Db.Disconnected.Iter().Single();
                if (row.Identity != firstIdentity)
                {
                    throw new Exception("Disconnected row identity did not match first connection identity");
                }

                disconnectedRowSeen = true;
            })
            .OnError((_, err) => throw err)
            .AddQuery(qb => qb.From.Disconnected())
            .Subscribe();
        reconnected = true;
    })
    .OnConnectError(err => throw err)
    .OnDisconnect((_, err) =>
    {
        if (err != null)
        {
            throw err;
        }
    })
    .Build();

FrameTickUntil(() => reconnected && disconnectedRowSeen, reconnectDb);
reconnectDb.Disconnect();

void FrameTickUntil(Func<bool> isComplete, DbConnection? connection = null, int timeoutSeconds = 20)
{
    var deadline = DateTime.UtcNow.AddSeconds(timeoutSeconds);
    connection ??= db;
    while (!isComplete())
    {
        connection.FrameTick();
        Thread.Sleep(25);
        if (DateTime.UtcNow > deadline)
        {
            throw new TimeoutException($"Timed out after {timeoutSeconds} seconds");
        }
    }
}
