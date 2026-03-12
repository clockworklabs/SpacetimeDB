using System.Threading;
using SpacetimeDB;
using SpacetimeDB.Types;

namespace RegressionTests.Shared;

internal static class RegressionTestHarness
{
    public static void RegisterUnhandledExceptionExitHandler()
    {
        AppDomain.CurrentDomain.UnhandledException += (_, eventArgs) =>
        {
            Log.Exception($"Unhandled exception: {eventArgs.ExceptionObject}");
            Environment.Exit(1);
        };
    }

    public static DbConnection ConnectToDatabase(
        string host,
        string databaseName,
        DbConnectionBuilder<DbConnection>.ConnectCallback onConnect,
        Action<Exception>? onConnectError = null,
        Action<Exception?>? onDisconnect = null
    )
    {
        return DbConnection
            .Builder()
            .WithUri(host)
            .WithDatabaseName(databaseName)
            .OnConnect(onConnect)
            .OnConnectError(err =>
            {
                if (onConnectError != null)
                {
                    onConnectError(err);
                    return;
                }

                throw err;
            })
            .OnDisconnect((_, err) =>
            {
                if (onDisconnect != null)
                {
                    onDisconnect(err);
                    return;
                }

                if (err != null)
                {
                    throw err;
                }

                throw new Exception("Unexpected disconnect");
            })
            .Build();
    }

    public static void FrameTickUntilComplete(
        DbConnection conn,
        Func<bool> isComplete,
        int timeoutSeconds,
        int sleepMilliseconds = 100,
        bool logStart = true
    )
    {
        if (logStart)
        {
            Log.Info("Starting timer");
        }

        var deadline = DateTime.UtcNow.AddSeconds(timeoutSeconds);
        while (!isComplete())
        {
            conn.FrameTick();
            Thread.Sleep(sleepMilliseconds);
            if (DateTime.UtcNow > deadline)
            {
                Log.Error($"Timeout, all events should have elapsed in {timeoutSeconds} seconds!");
                Environment.Exit(1);
            }
        }
    }

    public static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new Exception(message);
        }
    }
}
