using System.Diagnostics;
using RegressionTests.Shared;
using SpacetimeDB;
using SpacetimeDB.EventHandling;
using SpacetimeDB.Types;
using ExampleDataInsertHandler = SpacetimeDB.RemoteTableHandleBase<SpacetimeDB.Types.EventContext, SpacetimeDB.Types.ExampleData>.RowEventHandler;
#if SAPPY
using SpacetimeDB.SappyIntegration;
#endif

const string DefaultHost = "http://localhost:3000";
const string DefaultDatabase = "event-handling-bench";
var host = Environment.GetEnvironmentVariable("SPACETIMEDB_SERVER_URL") ?? DefaultHost;
var database = Environment.GetEnvironmentVariable("SPACETIMEDB_DATABASE") ?? DefaultDatabase;
var backend = ParseBackend(Environment.GetEnvironmentVariable("SPACETIMEDB_EVENT_BACKEND") ?? "all");

var scenarios = new[]
{
    new Scenario("few-subscriptions-many-updates", Subscriptions: 10, FirstUpdates: 1_000),
    new Scenario("many-subscriptions-some-updates", Subscriptions: 1_000, FirstUpdates: 10),
    new Scenario("many-subscriptions-many-updates", Subscriptions: 1_000, FirstUpdates: 1_000),
    new Scenario("many-subscriptions-some-updates-many-unsubscriptions", Subscriptions: 1_000, FirstUpdates: 10, Unsubscriptions: 1_000),
    new Scenario("many-subscriptions-some-updates-many-unsubscriptions-resubscriptions-some-updates", Subscriptions: 1_000, FirstUpdates: 10, Unsubscriptions: 1_000, Resubscriptions: 1_000, SecondUpdates: 10),
};

RegressionTestHarness.RegisterUnhandledExceptionExitHandler();

Console.WriteLine($"Host: {host}");
Console.WriteLine($"Database: {database}");
Console.WriteLine("| Backend | Scenario | Subscriptions | First updates | Unsubscriptions | Resubscriptions | Second updates | Subscribe ms | First updates ms | Unsubscribe ms | Resubscribe ms | Second updates ms | Listener calls |");
Console.WriteLine("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");

foreach (var backendKind in ExpandBackends(backend))
{
    ConfigureBackend(backendKind);

    foreach (var scenario in scenarios)
    {
        using var runner = new BenchmarkRunner(host, database, backendKind, scenario);
        var result = runner.Run();
        Console.WriteLine(
            $"| {backendKind} | {scenario.Name} | {scenario.Subscriptions} | {scenario.FirstUpdates} | {scenario.Unsubscriptions} | {scenario.Resubscriptions} | {scenario.SecondUpdates} | " +
            $"{result.Subscribe.TotalMilliseconds:F3} | {result.FirstUpdates.TotalMilliseconds:F3} | {result.Unsubscribe.TotalMilliseconds:F3} | {result.Resubscribe.TotalMilliseconds:F3} | {result.SecondUpdates.TotalMilliseconds:F3} | {result.ListenerCalls} |"
        );
    }
}

static BackendKind ParseBackend(string value) =>
    value.Trim().ToLowerInvariant() switch
    {
        "all" => BackendKind.All,
        "native" => BackendKind.Native,
        "custom" or "basic" => BackendKind.Custom,
        "sappy" => BackendKind.Sappy,
        _ => throw new ArgumentOutOfRangeException(nameof(value), value, "Expected all, native, custom, basic, or sappy."),
    };

static IEnumerable<BackendKind> ExpandBackends(BackendKind backend)
{
    if (backend != BackendKind.All)
    {
        yield return backend;
        yield break;
    }

    yield return BackendKind.Native;
    yield return BackendKind.Custom;
#if SAPPY
    yield return BackendKind.Sappy;
#endif
}

static void ConfigureBackend(BackendKind backend)
{
    switch (backend)
    {
        case BackendKind.Native:
            Backend.UseNativeEvents();
            return;
        case BackendKind.Custom:
            Backend.UseCustomListeners();
            return;
        case BackendKind.Sappy:
#if SAPPY
            Backend.UseCustomListeners(new SappyEventListenersFactory());
            return;
#else
            throw new InvalidOperationException("The Sappy benchmark requires compiling with SAPPY=1 inside a project that references Sappy.");
#endif
        default:
            throw new ArgumentOutOfRangeException(nameof(backend), backend, null);
    }
}

internal enum BackendKind
{
    All,
    Native,
    Custom,
    Sappy,
}

internal sealed record Scenario(
    string Name,
    int Subscriptions,
    int FirstUpdates,
    int Unsubscriptions = 0,
    int Resubscriptions = 0,
    int SecondUpdates = 0
);

internal readonly record struct BenchmarkResult(
    TimeSpan Subscribe,
    TimeSpan FirstUpdates,
    TimeSpan Unsubscribe,
    TimeSpan Resubscribe,
    TimeSpan SecondUpdates,
    long ListenerCalls
);

internal static class BenchmarkSettings
{
    public const int TimeoutSeconds = 120;
    public const int FrameSleepMilliseconds = 1;
}

internal sealed class BenchmarkRunner : IDisposable
{
    private readonly string _host;
    private readonly string _database;
    private readonly BackendKind _backend;
    private readonly Scenario _scenario;
    private readonly ExampleDataInsertHandler[] _listeners;
    private readonly Listener[] _listenerTargets;
    private readonly object _lock = new();
    private DbConnection _conn = null!;
    private SubscriptionHandle? _subscription;
    private long _listenerCalls;
    private long _targetListenerCalls;
    private bool _connected;
    private bool _subscriptionApplied;
    private Exception? _error;
    private uint _nextId;

    public BenchmarkRunner(string host, string database, BackendKind backend, Scenario scenario)
    {
        _host = host;
        _database = database;
        _backend = backend;
        _scenario = scenario;
        _listeners = new ExampleDataInsertHandler[scenario.Subscriptions];
        _listenerTargets = new Listener[scenario.Subscriptions];

        var idBase = unchecked((uint)HashCode.Combine(Environment.ProcessId, DateTime.UtcNow.Ticks, backend, scenario.Name));
        _nextId = idBase == 0 ? 1 : idBase;

        for (var i = 0; i < _listeners.Length; i++)
        {
            _listenerTargets[i] = new Listener(this);
            _listeners[i] = _listenerTargets[i].OnExampleDataInsert;
        }
    }

    public BenchmarkResult Run()
    {
        Connect();
        Subscribe();

        var subscribe = Time(() =>
        {
            foreach (var listener in _listeners)
            {
                _conn.Db.ExampleData.OnInsert += listener;
            }
        });

        var firstUpdates = TimeUpdates(_scenario.FirstUpdates, _scenario.Subscriptions);

        var unsubscribe = Time(() =>
        {
            for (var i = 0; i < _scenario.Unsubscriptions; i++)
            {
                _conn.Db.ExampleData.OnInsert -= _listeners[i];
            }
        });

        var resubscribe = Time(() =>
        {
            for (var i = 0; i < _scenario.Resubscriptions; i++)
            {
                _conn.Db.ExampleData.OnInsert += _listeners[i];
            }
        });

        var activeAfterResubscribe = _scenario.Subscriptions - _scenario.Unsubscriptions + _scenario.Resubscriptions;
        var secondUpdates = _scenario.SecondUpdates > 0 ? TimeUpdates(_scenario.SecondUpdates, activeAfterResubscribe) : TimeSpan.Zero;

        return new BenchmarkResult(
            subscribe,
            firstUpdates,
            unsubscribe,
            resubscribe,
            secondUpdates,
            Interlocked.Read(ref _listenerCalls)
        );
    }

    private void Connect()
    {
        _conn = RegressionTestHarness.ConnectToDatabase(
            _host,
            _database,
            (conn, _, _) =>
            {
                lock (_lock)
                {
                    _connected = true;
                }
            },
            error => RecordError(error),
            error =>
            {
                if (error != null)
                {
                    RecordError(error);
                }
            }
        );

        TickUntil(() => _connected, "connect");
    }

    private void Subscribe()
    {
        _subscription = _conn.SubscriptionBuilder()
            .OnApplied(_ =>
            {
                lock (_lock)
                {
                    _subscriptionApplied = true;
                }
            })
            .OnError((_, error) => RecordError(error))
            .AddQuery(q => q.From.ExampleData())
            .Subscribe();

        TickUntil(() => _subscriptionApplied, "subscription applied");
    }

    private TimeSpan TimeUpdates(int updateCount, int activeSubscriptions)
    {
        if (updateCount <= 0)
        {
            return TimeSpan.Zero;
        }

        var expectedCalls = activeSubscriptions * updateCount;
        var before = Interlocked.Read(ref _listenerCalls);
        Interlocked.Exchange(ref _targetListenerCalls, before + expectedCalls);

        return Time(() =>
        {
            for (var i = 0; i < updateCount; i++)
            {
                _conn.Reducers.Add(NextId(), (uint)i);
            }

            TickUntil(() => Interlocked.Read(ref _listenerCalls) >= Interlocked.Read(ref _targetListenerCalls), $"{updateCount} updates for {_scenario.Name}/{_backend}");
        });
    }

    private uint NextId()
    {
        var id = _nextId++;
        if (id == 0)
        {
            id = _nextId++;
        }

        return id;
    }

    private void RecordInsert()
    {
        Interlocked.Increment(ref _listenerCalls);
    }

    private TimeSpan Time(Action action)
    {
        var stopwatch = Stopwatch.StartNew();
        action();
        stopwatch.Stop();
        return stopwatch.Elapsed;
    }

    private void TickUntil(Func<bool> complete, string phase)
    {
        var deadline = DateTime.UtcNow.AddSeconds(BenchmarkSettings.TimeoutSeconds);
        while (!complete())
        {
            ThrowIfError();
            _conn.FrameTick();
            Thread.Sleep(BenchmarkSettings.FrameSleepMilliseconds);

            if (DateTime.UtcNow > deadline)
            {
                throw new TimeoutException($"Timed out waiting for {phase}.");
            }
        }

        ThrowIfError();
    }

    private void RecordError(Exception error)
    {
        lock (_lock)
        {
            _error ??= error;
        }
    }

    private void ThrowIfError()
    {
        lock (_lock)
        {
            if (_error != null)
            {
                throw new InvalidOperationException("Benchmark connection failed.", _error);
            }
        }
    }

    public void Dispose()
    {
        _subscription?.UnsubscribeThen(_ => { });
        _conn?.Disconnect();
    }

    private sealed class Listener
    {
        private readonly BenchmarkRunner _runner;

        public Listener(BenchmarkRunner runner)
        {
            _runner = runner;
        }

        public void OnExampleDataInsert(EventContext ctx, ExampleData row)
        {
            _runner.RecordInsert();
        }
    }
}
