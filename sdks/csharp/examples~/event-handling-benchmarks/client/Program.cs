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
const int DefaultWarmups = 1;
const int DefaultIterations = 3;

var host = Environment.GetEnvironmentVariable("SPACETIMEDB_SERVER_URL") ?? DefaultHost;
var database = Environment.GetEnvironmentVariable("SPACETIMEDB_DATABASE") ?? DefaultDatabase;
var backend = ParseBackend(Environment.GetEnvironmentVariable("SPACETIMEDB_EVENT_BACKEND") ?? "all");
var warmups = ParseNonNegativeEnv("SPACETIMEDB_BENCHMARK_WARMUPS", DefaultWarmups);
var iterations = ParsePositiveEnv("SPACETIMEDB_BENCHMARK_ITERATIONS", DefaultIterations);
var summaries = new List<BenchmarkSummary>();

var scenarios = new[]
{
    new Scenario("few-subscriptions-many-updates", Subscriptions: 10, FirstUpdates: 1_000),
    new Scenario("many-subscriptions-some-updates", Subscriptions: 1_000, FirstUpdates: 10),
    new Scenario("many-subscriptions-many-updates", Subscriptions: 1_000, FirstUpdates: 1_000),
    new Scenario("many-subscriptions-some-updates-many-unsubscriptions", Subscriptions: 1_000, FirstUpdates: 10, Unsubscriptions: 1_000),
    new Scenario("many-subscriptions-some-updates-many-unsubscriptions-resubscriptions-some-updates", Subscriptions: 1_000, FirstUpdates: 10, Unsubscriptions: 1_000, Resubscriptions: 1_000, SecondUpdates: 10),
    new Scenario("many-subscriptions-some-updates-many-unsubscriptions-resubscriptions-some-updates", Subscriptions: 1_000, FirstUpdates: 1_000, Unsubscriptions: 1_000, Resubscriptions: 1_000, SecondUpdates: 1_000),
};

RegressionTestHarness.RegisterUnhandledExceptionExitHandler();

Console.WriteLine($"Host: {host}");
Console.WriteLine($"Database: {database}");
Console.WriteLine($"Warmups: {warmups}");
Console.WriteLine($"Iterations: {iterations}");
Console.WriteLine();
Console.WriteLine("Per-iteration timings:");
Console.WriteLine("| Backend | Scenario | Iteration | Subscriptions | First updates | Unsubscriptions | Resubscriptions | Second updates | Subscribe ms | Subscribe bytes | First updates ms | First updates bytes | Unsubscribe ms | Unsubscribe bytes | Resubscribe ms | Resubscribe bytes | Second updates ms | Second updates bytes | Listener calls |");
Console.WriteLine("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");

foreach (var backendKind in ExpandBackends(backend))
{
    ConfigureBackend(backendKind);

    foreach (var scenario in scenarios)
    {
        for (var i = 0; i < warmups; i++)
        {
            RunOne(host, database, backendKind, scenario);
        }

        var results = new BenchmarkResult[iterations];
        for (var i = 0; i < iterations; i++)
        {
            GC.Collect();
            GC.WaitForPendingFinalizers();
            GC.Collect();

            results[i] = RunOne(host, database, backendKind, scenario);
            PrintResult(backendKind, scenario, i + 1, results[i]);
        }

        summaries.Add(BenchmarkSummary.From(backendKind, scenario, results));
    }
}

Console.WriteLine();
Console.WriteLine("Summary timings:");
Console.WriteLine("| Backend | Scenario | Subscribe ms avg | Subscribe bytes avg | First updates ms avg | First updates bytes avg | Unsubscribe ms avg | Unsubscribe bytes avg | Resubscribe ms avg | Resubscribe bytes avg | Second updates ms avg | Second updates bytes avg |");
Console.WriteLine("| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
foreach (var summary in summaries)
{
    Console.WriteLine(
        $"| {summary.Backend} | {summary.Scenario.Name} | " +
        $"{summary.Subscribe.MeanMilliseconds:F3} | {summary.Subscribe.MeanAllocatedBytes:F0} | " +
        $"{summary.FirstUpdates.MeanMilliseconds:F3} | {summary.FirstUpdates.MeanAllocatedBytes:F0} | " +
        $"{summary.Unsubscribe.MeanMilliseconds:F3} | {summary.Unsubscribe.MeanAllocatedBytes:F0} | " +
        $"{summary.Resubscribe.MeanMilliseconds:F3} | {summary.Resubscribe.MeanAllocatedBytes:F0} | " +
        $"{summary.SecondUpdates.MeanMilliseconds:F3} | {summary.SecondUpdates.MeanAllocatedBytes:F0} |"
    );
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

static BenchmarkResult RunOne(string host, string database, BackendKind backendKind, Scenario scenario)
{
    using var runner = new BenchmarkRunner(host, database, backendKind, scenario);
    return runner.Run();
}

static void PrintResult(BackendKind backendKind, Scenario scenario, int iteration, BenchmarkResult result)
{
    Console.WriteLine(
        $"| {backendKind} | {scenario.Name} | {iteration} | {scenario.Subscriptions} | {scenario.FirstUpdates} | {scenario.Unsubscriptions} | {scenario.Resubscriptions} | {scenario.SecondUpdates} | " +
        $"{result.Subscribe.Elapsed.TotalMilliseconds:F3} | {result.Subscribe.AllocatedBytes} | " +
        $"{result.FirstUpdates.Elapsed.TotalMilliseconds:F3} | {result.FirstUpdates.AllocatedBytes} | " +
        $"{result.Unsubscribe.Elapsed.TotalMilliseconds:F3} | {result.Unsubscribe.AllocatedBytes} | " +
        $"{result.Resubscribe.Elapsed.TotalMilliseconds:F3} | {result.Resubscribe.AllocatedBytes} | " +
        $"{result.SecondUpdates.Elapsed.TotalMilliseconds:F3} | {result.SecondUpdates.AllocatedBytes} | {result.ListenerCalls} |"
    );
}

static int ParseNonNegativeEnv(string name, int defaultValue)
{
    var value = Environment.GetEnvironmentVariable(name);
    if (string.IsNullOrWhiteSpace(value))
    {
        return defaultValue;
    }

    if (int.TryParse(value, out var parsed) && parsed >= 0)
    {
        return parsed;
    }

    throw new ArgumentOutOfRangeException(name, value, "Expected a non-negative integer.");
}

static int ParsePositiveEnv(string name, int defaultValue)
{
    var value = Environment.GetEnvironmentVariable(name);
    if (string.IsNullOrWhiteSpace(value))
    {
        return defaultValue;
    }

    if (int.TryParse(value, out var parsed) && parsed > 0)
    {
        return parsed;
    }

    throw new ArgumentOutOfRangeException(name, value, "Expected a positive integer.");
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
    Measurement Subscribe,
    Measurement FirstUpdates,
    Measurement Unsubscribe,
    Measurement Resubscribe,
    Measurement SecondUpdates,
    long ListenerCalls
);

internal readonly record struct Measurement(TimeSpan Elapsed, long AllocatedBytes)
{
    public static Measurement Zero { get; } = new(TimeSpan.Zero, 0);
}

internal readonly record struct Stats(
    double MeanMilliseconds,
    double MinMilliseconds,
    double MaxMilliseconds,
    double MeanAllocatedBytes,
    long MinAllocatedBytes,
    long MaxAllocatedBytes
)
{
    public static Stats From(IEnumerable<Measurement> values)
    {
        var measurements = values.ToArray();
        var milliseconds = measurements.Select(value => value.Elapsed.TotalMilliseconds).ToArray();
        var allocatedBytes = measurements.Select(value => value.AllocatedBytes).ToArray();
        return new Stats(
            milliseconds.Average(),
            milliseconds.Min(),
            milliseconds.Max(),
            allocatedBytes.Average(),
            allocatedBytes.Min(),
            allocatedBytes.Max()
        );
    }
}

internal readonly record struct BenchmarkSummary(
    BackendKind Backend,
    Scenario Scenario,
    Stats Subscribe,
    Stats FirstUpdates,
    Stats Unsubscribe,
    Stats Resubscribe,
    Stats SecondUpdates
)
{
    public static BenchmarkSummary From(BackendKind backend, Scenario scenario, BenchmarkResult[] results) =>
        new(
            backend,
            scenario,
            Stats.From(results.Select(r => r.Subscribe)),
            Stats.From(results.Select(r => r.FirstUpdates)),
            Stats.From(results.Select(r => r.Unsubscribe)),
            Stats.From(results.Select(r => r.Resubscribe)),
            Stats.From(results.Select(r => r.SecondUpdates))
        );
}

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
        var secondUpdates = _scenario.SecondUpdates > 0 ? TimeUpdates(_scenario.SecondUpdates, activeAfterResubscribe) : Measurement.Zero;

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

    private Measurement TimeUpdates(int updateCount, int activeSubscriptions)
    {
        if (updateCount <= 0)
        {
            return Measurement.Zero;
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

    private Measurement Time(Action action)
    {
        var allocatedBefore = GC.GetTotalAllocatedBytes(precise: true);
        var stopwatch = Stopwatch.StartNew();
        action();
        stopwatch.Stop();
        var allocatedAfter = GC.GetTotalAllocatedBytes(precise: true);
        return new Measurement(stopwatch.Elapsed, allocatedAfter - allocatedBefore);
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
