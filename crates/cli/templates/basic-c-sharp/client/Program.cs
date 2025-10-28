using System;
using System.Threading;
using SpacetimeDB;
using SpacetimeDB.Types;

/// The URI of the SpacetimeDB instance hosting our chat module.
string HOST = Environment.GetEnvironmentVariable("SPACETIMEDB_HOST") ?? "http://localhost:3000";

/// The module name we chose when we published our module.
string DB_NAME = Environment.GetEnvironmentVariable("SPACETIMEDB_DB_NAME") ?? "my-db";

void Main()
{
    // Initialize the AuthToken module
    AuthToken.Init(".spacetime_csharp");

    // Build and connect to the database
    var conn = DbConnection.Builder()
        .WithUri(HOST)
        .WithModuleName(DB_NAME)
        .WithToken(AuthToken.Token)
        .OnConnect(OnConnected)
        .OnConnectError(OnConnectError)
        .OnDisconnect(OnDisconnected)
        .Build();

    // Keep the connection alive and process updates
    try
    {
        while (true)
        {
            conn.FrameTick();
            Thread.Sleep(100);
        }
    }
    finally
    {
        conn.Disconnect();
    }
}

void OnConnected(DbConnection conn, Identity identity, string authToken)
{
    Console.WriteLine($"Connected to {DB_NAME}");
    Console.WriteLine($"Identity: {identity}");

    // Save credentials for future sessions
    AuthToken.SaveToken(authToken);

    // Subscribe to all tables to receive updates
    conn.SubscriptionBuilder()
        .OnApplied(OnSubscriptionApplied)
        .SubscribeToAllTables();
}

void OnConnectError(Exception e)
{
    Console.WriteLine($"Connection error: {e.Message}");
}

void OnDisconnected(DbConnection conn, Exception? e)
{
    if (e != null)
    {
        Console.WriteLine($"Disconnected with error: {e.Message}");
    }
    else
    {
        Console.WriteLine("Disconnected");
    }
}

void OnSubscriptionApplied(SubscriptionEventContext ctx)
{
    Console.WriteLine("Subscription applied - ready to receive updates");
}

Main();
