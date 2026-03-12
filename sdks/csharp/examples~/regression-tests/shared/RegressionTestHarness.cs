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

    public static void RunNamedTests(string[] args, IReadOnlyDictionary<string, Action> tests)
    {
        if (args.Length > 1)
        {
            throw new ArgumentException("Pass zero args (run all) or a single test name.");
        }

        if (args.Length == 1)
        {
            var testName = args[0];
            if (!tests.TryGetValue(testName, out var test))
            {
                throw new ArgumentException($"Unknown test: {testName}");
            }

            Log.Info($"Running {testName}");
            test();
            return;
        }

        foreach (var (testName, test) in tests)
        {
            Log.Info($"Running {testName}");
            test();
        }
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

    public static void RunLiveConnectionTest(
        string host,
        string databaseName,
        string testName,
        int timeoutSeconds,
        Action<DbConnection, Action, Action<Exception>> start
    )
    {
        bool complete = false;
        bool disconnectExpected = false;
        Exception? failure = null;

        void Pass() => complete = true;
        void Fail(Exception error) => failure ??= error;

        var conn = ConnectToDatabase(
            host,
            databaseName,
            (connected, _, _) =>
            {
                try
                {
                    start(connected, Pass, Fail);
                }
                catch (Exception ex)
                {
                    Fail(ex);
                }
            },
            onConnectError: Fail,
            onDisconnect: err =>
            {
                if (disconnectExpected)
                {
                    return;
                }

                if (err != null)
                {
                    Fail(err);
                    return;
                }

                if (!complete)
                {
                    Fail(new Exception($"Unexpected disconnect in {testName}"));
                }
            }
        );

        FrameTickUntilComplete(
            conn,
            () => complete || failure != null,
            timeoutSeconds,
            sleepMilliseconds: 10,
            logStart: false
        );

        disconnectExpected = true;
        if (conn.IsActive)
        {
            conn.Disconnect();
        }

        if (failure != null)
        {
            throw new Exception($"{testName} failed", failure);
        }
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

    public static void AssertReducerCommitted(string reducerName, ReducerEventContext ctx)
    {
        switch (ctx.Event.Status)
        {
            case Status.Committed:
                return;
            case Status.Failed(var reason):
                throw new Exception($"`{reducerName}` reducer returned error: {reason}");
            case Status.OutOfEnergy(var _):
                throw new Exception($"`{reducerName}` reducer ran out of energy");
            default:
                throw new Exception(
                    $"`{reducerName}` reducer returned unexpected status: {ctx.Event.Status}"
                );
        }
    }
}
