using System.Collections.Concurrent;
using System.Diagnostics;
using System.Reflection;
using System.Text;
using SpacetimeDB;
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;
using Xunit;

namespace SpacetimeDB.Tests;

[CollectionDefinition("SDK connections", DisableParallelization = true)]
public class ConnectionTestCollection { }

[Collection("SDK connections")]
public partial class ReconnectTests
{
    [SpacetimeDB.Type]
    public partial class Row
    {
        public uint Id;
        public string Value = "";
    }

    [SpacetimeDB.Type]
    public partial class Args : IReducerArgs, IProcedureArgs
    {
        string IReducerArgs.ReducerName => "test";
        string IProcedureArgs.ProcedureName => "test";
    }

    public sealed class Context : IEventContext, ISubscriptionEventContext, IErrorContext, IReducerEventContext, IProcedureEventContext
    {
        public Exception Event { get; init; } = new Exception();
        public ReducerEvent<Args>? ReducerEvent;
    }

    public sealed class TestTable : RemoteTableHandle<Context, Row>
    {
        private readonly string name;
        private readonly bool primaryKey;
        public override string RemoteTableName => name;
        protected override object? GetPrimaryKey(Row row) => primaryKey ? row.Id : null;
        public TestTable(IDbConnection conn, string name, bool primaryKey) : base(conn)
        {
            this.name = name;
            this.primaryKey = primaryKey;
        }
    }

    public sealed class Tables : RemoteTablesBase
    {
        public readonly TestTable Keyed;
        public readonly TestTable Unkeyed;
        public Tables(IDbConnection conn)
        {
            AddTable(Keyed = new(conn, "keyed", true));
            AddTable(Unkeyed = new(conn, "unkeyed", false));
        }
    }

    public sealed class Connection : DbConnectionBase<Connection, Tables, Args>, IDisposable
    {
        public override Tables Db { get; }
        internal readonly List<FakeSocket> Sockets = new();
        internal readonly List<(string Kind, Exception? Error, NextReconnect? Next)> Events = new();
        internal readonly List<Status> ReducerResults = new();
        internal double Now;
        internal FakeSocket Socket => Sockets[^1];
        internal int Connects;
        internal int CallbackThread;

        public Connection()
        {
            Db = new(this);
            SocketFactory = () =>
            {
                var socket = new FakeSocket();
                Sockets.Add(socket);
                return socket;
            };
            ReconnectClock = () => Now;
            ((IDbConnection)this).AddOnConnect((_, _) => { Connects++; CallbackThread = Environment.CurrentManagedThreadId; });
            ((IDbConnection)this).AddOnDisconnect((error, next) => Events.Add(("disconnect", error, next)));
            ((IDbConnection)this).AddOnConnectError((error, next) => Events.Add(("error", error, next)));
        }

        protected override IEventContext ToEventContext(Event<Args> e) => new Context();
        protected override IReducerEventContext ToReducerEventContext(ReducerEvent<Args> e) => new Context { ReducerEvent = e };
        protected override ISubscriptionEventContext MakeSubscriptionEventContext() => new Context();
        protected override IErrorContext ToErrorContext(Exception e) => new Context { Event = e };
        protected override IProcedureEventContext ToProcedureEventContext(ProcedureEvent e) => new Context();
        protected override bool Dispatch(IReducerEventContext context, Args reducer)
        {
            ReducerResults.Add(((Context)context).ReducerEvent!.Status);
            return true;
        }
        public void Dispose() => Disconnect();
    }

    internal sealed class FakeSocket : SpacetimeDB.WebSocket
    {
        internal bool Open;
        internal readonly ConcurrentQueue<ClientMessage> Sent = new();
        internal string? Token;
        internal ConnectionId? Session;
        internal ConnectionId Id;
        internal volatile bool Started;
        public override bool IsConnected => Open;
        internal FakeSocket() : base(new ConnectOptions { Protocol = "v2.bsatn.spacetimedb" }) { }
        public override Task Connect(string? auth, string host, string database, ConnectionId id, Compression compression,
            bool light, bool? confirmed, ConnectionId? sessionId = null)
        {
            Token = auth;
            Session = sessionId;
            Id = id;
            Started = true;
            return Task.CompletedTask;
        }
        public override void Send(ClientMessage message) => Sent.Enqueue(message);
        public override void Abort() => Open = false;
        private T Handler<T>(string name) where T : Delegate =>
            (T)typeof(SpacetimeDB.WebSocket).GetField(name, BindingFlags.Instance | BindingFlags.NonPublic)!.GetValue(this)!;
        internal void Receive(ServerMessage message)
        {
            var bytes = IStructuralReadWrite.ToBytes(new ServerMessage.BSATN(), message);
            Handler<MessageEventHandler>("OnMessage")([0, .. bytes], DateTime.UtcNow);
        }
        internal void Lose(Exception? error = null)
        {
            Open = false;
            Handler<CloseEventHandler>("OnClose")(error);
        }
        internal void Fail(Exception error) => Handler<ConnectErrorEventHandler>("OnConnectError")(error);
    }

    private sealed class Handle : SubscriptionHandleBase<Context, Context>
    {
        internal Handle(Connection conn, string query = "SELECT * FROM keyed") : base(conn, null, null, [query]) { }
    }

    private sealed class TrackingHandle : ISubscriptionHandle
    {
        internal int Applied;
        internal int Errors;
        internal int Ended;
        internal QuerySetId Id = new();
        public void RebindQuerySetId(QuerySetId id) => Id = id;
        public void OnApplied(ISubscriptionEventContext ctx) => Applied++;
        public void OnError(IErrorContext ctx) => Errors++;
        public void OnEnded(ISubscriptionEventContext ctx) => Ended++;
    }

    private static Connection Create(bool automatic = true, string? token = null, Func<Task<string>>? provider = null, AutomaticReconnectOptions? options = null)
    {
        var builder = Connection.Builder().WithUri("ws://localhost").WithDatabaseName("test").WithToken(token);
        if (automatic) builder.WithAutomaticReconnect(options);
        if (provider != null) builder.WithTokenProvider(provider);
        var conn = builder.Build();
        Pump(conn, () => conn.Socket.Started);
        return conn;
    }

    private static void Pump(Connection conn, Func<bool> done)
    {
        var timer = Stopwatch.StartNew();
        do
        {
            conn.FrameTick();
            if (done()) return;
            Thread.Sleep(1);
        } while (timer.Elapsed < TimeSpan.FromSeconds(5));
        Assert.Fail("Timed out waiting for SDK work.");
    }

    private static readonly Identity identity = Identity.From(new byte[32]);
    private static void Establish(Connection conn, string token = "issued-token", Identity? asIdentity = null)
    {
        var count = conn.Connects;
        var errors = conn.Events.Count;
        conn.Socket.Open = true;
        conn.Socket.Receive(new ServerMessage.InitialConnection(new(asIdentity ?? identity, conn.ConnectionId, token)));
        Pump(conn, () => conn.Connects > count || conn.Events.Count > errors && conn.Events[^1].Next == null);
    }

    private static void Drop(Connection conn)
    {
        var count = conn.Events.Count;
        conn.Socket.Lose(new IOException("dropped"));
        Pump(conn, () => conn.Events.Count > count);
    }

    private static void Retry(Connection conn)
    {
        var count = conn.Sockets.Count;
        conn.Now += 31;
        Pump(conn, () => conn.Sockets.Count > count && conn.Socket.Started);
    }

    private static TrackingHandle Subscribe(Connection conn, string query = "SELECT * FROM keyed")
    {
        var handle = new TrackingHandle();
        handle.Id = ((IDbConnection)conn).Subscribe(handle, [query])!;
        return handle;
    }

    private static BsatnRowList Rows(params Row[] rows)
    {
        var bytes = new List<byte>();
        var offsets = new List<ulong>();
        foreach (var row in rows)
        {
            offsets.Add((ulong)bytes.Count);
            bytes.AddRange(IStructuralReadWrite.ToBytes(row));
        }
        return new(new RowSizeHint.RowOffsets(offsets), bytes);
    }

    private static QueryRows Query(string table, params Row[] rows) => new(new() { new(table, Rows(rows)) });

    private static void Apply(Connection conn, TrackingHandle handle, QueryRows rows)
    {
        var count = handle.Applied;
        var sent = conn.Socket.Sent.OfType<ClientMessage.Subscribe>().Last().Subscribe_;
        conn.Socket.Receive(new ServerMessage.SubscribeApplied(new(sent.RequestId, handle.Id, rows)));
        Pump(conn, () => handle.Applied > count);
    }

    private static SubscribeBatch Batch(Connection conn) => conn.Socket.Sent.OfType<ClientMessage.SubscribeBatch>().Single().SubscribeBatch_;
    [Fact]
    public void ReusesIdentitySessionAndHandlesWithNewConnectionIds()
    {
        using var conn = Create();
        Establish(conn);
        var socket = conn.Socket;
        var table = conn.Db.Keyed;
        var first = conn.ConnectionId;
        var handle = Subscribe(conn);
        Apply(conn, handle, Query("keyed", new Row { Id = 1, Value = "unchanged" }));
        var oldId = handle.Id;
        var inserts = 0;
        var updates = 0;
        var deletes = 0;
        table.OnInsert += (_, _) => inserts++;
        table.OnUpdate += (_, _, _) => updates++;
        table.OnDelete += (_, _) => deletes++;
        Drop(conn);
        Assert.True(conn.IsReconnecting);
        Assert.False(conn.IsActive);
        Assert.Equal(1, table.Count);
        Assert.Equal(1, conn.Events[^1].Next!.Value.Attempt);
        Retry(conn);
        Assert.Equal("issued-token", conn.Socket.Token);
        Assert.Equal(socket.Session, conn.Socket.Session);
        Assert.NotEqual(first, conn.ConnectionId);
        Establish(conn);
        Assert.NotEqual(oldId.Id, handle.Id.Id);
        conn.Socket.Receive(new ServerMessage.SubscribeBatchApplied(new(Batch(conn).RequestId,
            new() { new(handle.Id, new SubscribeSetOutcome.Applied(Query("keyed", new Row { Id = 1, Value = "unchanged" }))) })));
        Pump(conn, () => handle.Applied == 2);
        Assert.Same(table, conn.Db.Keyed);
        Assert.Equal(0, inserts + updates + deletes);
        Assert.Equal(Environment.CurrentManagedThreadId, conn.CallbackThread);
        Drop(conn);
        Assert.Equal(1, conn.Events[^1].Next!.Value.Attempt);
    }

    [Theory]
    [InlineData(false)]
    [InlineData(true)]
    public void InitialFailureNeverRetries(bool automatic)
    {
        using var conn = Create(automatic);
        conn.Socket.Fail(new IOException("unreachable"));
        Pump(conn, () => conn.Events.Count == 1);
        Assert.Equal("error", conn.Events[0].Kind);
        Assert.Null(conn.Events[0].Next);
        conn.Now += 100;
        conn.FrameTick();
        Assert.Single(conn.Sockets);
        Assert.False(conn.IsReconnecting);
    }

    [Fact]
    public void OptOutNeverReplaysOrSendsSessionId()
    {
        using var conn = Create(false);
        Establish(conn);
        Assert.Null(conn.Socket.Session);
        Drop(conn);
        Assert.Null(conn.Events[^1].Next);
        Assert.False(conn.IsReconnecting);
    }

    [Fact]
    public void DuplicateLossAndStaleMessagesCannotAffectRetry()
    {
        using var conn = Create();
        Establish(conn);
        var old = conn.Socket;
        old.Lose();
        old.Fail(new IOException());
        Pump(conn, () => conn.Events.Count > 0);
        Retry(conn);
        old.Receive(new ServerMessage.InitialConnection(new(identity, old.Id, "wrong")));
        old.Lose();
        Establish(conn);
        Assert.Equal(2, conn.Connects);
        Assert.Single(conn.Events);
        Assert.Equal("issued-token", conn.Socket.Token);
    }

    [Fact]
    public void RetriesBackOffAndCancelFromCallback()
    {
        using var conn = Create();
        Establish(conn);
        Drop(conn);
        Retry(conn);
        conn.Socket.Fail(new IOException("offline"));
        Pump(conn, () => conn.Events.Count == 2);
        Assert.Equal("error", conn.Events[^1].Kind);
        Assert.Equal(2, conn.Events[^1].Next!.Value.Attempt);
        ((IDbConnection)conn).AddOnConnectError((_, _) => conn.Disconnect());
        Retry(conn);
        conn.Socket.Fail(new IOException("offline"));
        Pump(conn, () => conn.Events.Count == 4);
        Assert.Equal("disconnect", conn.Events[^1].Kind);
        Assert.Null(conn.Events[^1].Error);
        Assert.Null(conn.Events[^1].Next);
        Assert.False(conn.IsReconnecting);
    }

    [Fact]
    public void IdentityChangeAndProtocolErrorsAreTerminal()
    {
        using var conn = Create();
        Establish(conn);
        Drop(conn);
        Retry(conn);
        var bytes = new byte[32];
        bytes[0] = 1;
        Establish(conn, asIdentity: Identity.From(bytes));
        Assert.Equal(1, conn.Connects);
        Assert.Equal("error", conn.Events[^1].Kind);
        Assert.IsType<ConnectionProtocolException>(conn.Events[^1].Error);
        Assert.Null(conn.Events[^1].Next);
        Assert.Equal(identity, conn.Identity);
    }

    [Fact]
    public void InFlightCallsHaveUnknownResultsAndOfflineCallsFailFast()
    {
        using var conn = Create();
        Establish(conn);
        Exception? procedureError = null;
        ((IDbConnection)conn).InternalCallReducer(new Args());
        ((IDbConnection)conn).InternalCallProcedure<Args, Row>(new(), (_, result) => procedureError = result.Error);
        var query = ((IDbConnection)conn).RemoteQuery<Row>("SELECT * FROM keyed");
        Drop(conn);
        Assert.IsType<Status.UnknownResult>(Assert.Single(conn.ReducerResults));
        Assert.IsType<UnknownResultException>(procedureError);
        Assert.IsType<UnknownResultException>(query.Exception!.InnerException);
        Assert.Throws<InvalidOperationException>(() => ((IDbConnection)conn).InternalCallReducer(new Args()));
        Assert.Throws<InvalidOperationException>(() => ((IDbConnection)conn).InternalCallProcedure<Args, Row>(new(), (_, _) => { }));
        Assert.IsType<InvalidOperationException>(((IDbConnection)conn).RemoteQuery<Row>("SELECT * FROM keyed").Exception!.InnerException);
    }

    [Fact]
    public void ReconciliationCombinesOverlapsNetChangesAndFailures()
    {
        using var conn = Create();
        Establish(conn);
        var first = Subscribe(conn);
        Apply(conn, first, Query("keyed", new Row { Id = 1, Value = "same" }, new Row { Id = 2, Value = "old" }, new Row { Id = 3 }));
        var second = Subscribe(conn);
        Apply(conn, second, Query("keyed", new Row { Id = 1, Value = "same" }));
        var failed = Subscribe(conn);
        Apply(conn, failed, Query("keyed", new Row { Id = 4 }));
        var changes = new List<string>();
        var callbackCounts = new List<int>();
        conn.Db.Keyed.OnInsert += (_, row) => { callbackCounts.Add(conn.Db.Keyed.Count); changes.Add($"insert:{row.Id}"); };
        conn.Db.Keyed.OnUpdate += (_, old, row) => changes.Add($"update:{old.Value}:{row.Value}");
        conn.Db.Keyed.OnDelete += (_, row) => changes.Add($"delete:{row.Id}");
        Drop(conn);
        Retry(conn);
        Establish(conn);
        var batch = Batch(conn);
        Assert.Equal(3, batch.Sets.Count);
        conn.Socket.Receive(new ServerMessage.SubscribeBatchApplied(new(batch.RequestId, new()
        {
            new(first.Id, new SubscribeSetOutcome.Applied(Query("keyed", new Row { Id = 1, Value = "same" }, new Row { Id = 2, Value = "new" }, new Row { Id = 5 }))),
            new(second.Id, new SubscribeSetOutcome.Applied(Query("keyed", new Row { Id = 1, Value = "same" }))),
            new(failed.Id, new SubscribeSetOutcome.Error("invalid query"))
        })));
        Pump(conn, () => failed.Errors == 1);
        Assert.Equal(new[] { "delete:3", "delete:4", "insert:5", "update:old:new" }, changes.OrderBy(x => x));
        Assert.All(callbackCounts, count => Assert.Equal(3, count));
        Assert.Equal(2, first.Applied);
        Assert.Equal(2, second.Applied);
        ((IDbConnection)conn).Unsubscribe(second.Id);
        conn.Socket.Receive(new ServerMessage.UnsubscribeApplied(new(0, second.Id, Query("keyed", new Row { Id = 1, Value = "same" })))) ;
        Pump(conn, () => second.Ended == 1);
        Assert.Equal(3, conn.Db.Keyed.Count);
    }

    [Fact]
    public void QueuedAndCancelledSubscriptionsAreReflectedInReplay()
    {
        using var conn = Create();
        Establish(conn);
        var old = Subscribe(conn);
        Apply(conn, old, Query("keyed", new Row { Id = 1 }));
        Drop(conn);
        ((IDbConnection)conn).Unsubscribe(old.Id);
        Assert.Equal(1, old.Ended);
        var added = Subscribe(conn, "SELECT * FROM keyed WHERE id = 2");
        var cancelled = new Handle(conn);
        cancelled.Unsubscribe();
        Assert.True(cancelled.IsEnded);
        Retry(conn);
        Establish(conn);
        Assert.Single(Batch(conn).Sets);
        conn.Socket.Receive(new ServerMessage.SubscribeBatchApplied(new(Batch(conn).RequestId, new()
        {
            new(added.Id, new SubscribeSetOutcome.Applied(Query("keyed", new Row { Id = 2 })))
        })));
        Pump(conn, () => added.Applied == 1);
        Assert.Equal(2u, conn.Db.Keyed.Iter().Single().Id);
    }

    [Fact]
    public void EmptyReplayClearsCacheAndBadBatchIsTerminal()
    {
        using var conn = Create();
        Establish(conn);
        var handle = Subscribe(conn);
        Apply(conn, handle, Query("keyed", new Row { Id = 1 }));
        Drop(conn);
        ((IDbConnection)conn).Unsubscribe(handle.Id);
        Retry(conn);
        Establish(conn);
        Assert.Equal(0, conn.Db.Keyed.Count);
        Assert.Empty(conn.Socket.Sent);
        conn.Socket.Receive(new ServerMessage.SubscribeBatchApplied(new(123, new())));
        Pump(conn, () => conn.Events.Count == 2);
        Assert.Null(conn.Events[^1].Next);
        Assert.IsType<ConnectionProtocolException>(conn.Events[^1].Error);
    }

    [Fact]
    public void TokenProviderRefreshesOnlyWhenNeededAndCanBeCancelled()
    {
        var providerCalls = 0;
        var source = new TaskCompletionSource<string>();
        using var conn = Create(provider: () => { providerCalls++; return source.Task; });
        Assert.Equal(0, providerCalls);
        Establish(conn);
        Drop(conn);
        conn.Now += 31;
        conn.FrameTick();
        Assert.Equal(1, providerCalls);
        Assert.True(conn.IsReconnecting);
        conn.Disconnect();
        source.SetResult("fresh-token");
        conn.FrameTick();
        Assert.Single(conn.Sockets);
    }

    [Fact]
    public void ProviderFailureRetriesAndFreshTokenRejectionStops()
    {
        var calls = 0;
        using var conn = Create(provider: () => ++calls == 1
            ? Task.FromException<string>(new IOException("provider unavailable")) : Task.FromResult("fresh-token"));
        Establish(conn);
        Drop(conn);
        conn.Now += 31;
        Pump(conn, () => conn.Events.Count == 2);
        Assert.Equal(2, conn.Events[^1].Next!.Value.Attempt);
        Retry(conn);
        Assert.Equal("fresh-token", conn.Socket.Token);
        conn.Socket.Fail(new SpacetimeDB.WebSocket.ConnectException(401));
        Pump(conn, () => conn.Events.Count == 3);
        Assert.Null(conn.Events[^1].Next);
    }

    [Fact]
    public void RejectedRetainedTokenForcesRefreshDespiteDistantExpiry()
    {
        var calls = 0;
        var token = Jwt(DateTimeOffset.UtcNow.ToUnixTimeSeconds() + 3600, DateTimeOffset.UtcNow.ToUnixTimeSeconds());
        using var conn = Create(token: token, provider: () => { calls++; return Task.FromResult("fresh-token"); });
        Establish(conn);
        Drop(conn);
        Retry(conn);
        Assert.Equal(0, calls);
        Assert.Equal(token, conn.Socket.Token);
        conn.Socket.Fail(new SpacetimeDB.WebSocket.ConnectException(401));
        Pump(conn, () => conn.Events.Count == 2);
        Retry(conn);
        Assert.Equal(1, calls);
        Assert.Equal("fresh-token", conn.Socket.Token);
        Establish(conn);
        Assert.Equal(2, conn.Connects);
    }

    [Fact]
    public void NoPrimaryKeyChangesProduceDeleteInsertAfterAtomicApply()
    {
        using var conn = Create();
        Establish(conn);
        var keyed = Subscribe(conn);
        Apply(conn, keyed, Query("keyed", new Row { Id = 1, Value = "old" }));
        var unkeyed = Subscribe(conn, "SELECT * FROM unkeyed");
        Apply(conn, unkeyed, Query("unkeyed", new Row { Id = 1, Value = "old" }));
        var changes = new List<string>();
        string? observedOtherTable = null;
        conn.Db.Keyed.OnUpdate += (_, _, _) => observedOtherTable = conn.Db.Unkeyed.Iter().Single().Value;
        conn.Db.Unkeyed.OnDelete += (_, row) => changes.Add($"delete:{row.Value}");
        conn.Db.Unkeyed.OnInsert += (_, row) => changes.Add($"insert:{row.Value}");
        Drop(conn);
        Retry(conn);
        Establish(conn);
        conn.Socket.Receive(new ServerMessage.SubscribeBatchApplied(new(Batch(conn).RequestId, new()
        {
            new(keyed.Id, new SubscribeSetOutcome.Applied(Query("keyed", new Row { Id = 1, Value = "new" }))),
            new(unkeyed.Id, new SubscribeSetOutcome.Applied(Query("unkeyed", new Row { Id = 1, Value = "new" })))
        })));
        Pump(conn, () => unkeyed.Applied == 2);
        Assert.Equal("new", observedOtherTable);
        Assert.Equal(new[] { "delete:old", "insert:new" }, changes.OrderBy(x => x));
    }

    [Theory]
    [InlineData("missing")]
    [InlineData("duplicate")]
    [InlineData("unknown")]
    [InlineData("request")]
    public void MalformedReplayNeverChangesTheCache(string kind)
    {
        using var conn = Create();
        Establish(conn);
        var handle = Subscribe(conn);
        Apply(conn, handle, Query("keyed", new Row { Id = 1 }));
        Drop(conn);
        Retry(conn);
        Establish(conn);
        var result = new SubscribeSetResult(handle.Id, new SubscribeSetOutcome.Applied(Query("keyed", new Row { Id = 2 })));
        var results = new List<SubscribeSetResult> { result };
        if (kind == "missing") results.Clear();
        if (kind == "duplicate") results.Add(result);
        if (kind == "unknown") result.QuerySetId = new(9999);
        conn.Socket.Receive(new ServerMessage.SubscribeBatchApplied(new(Batch(conn).RequestId + (kind == "request" ? 1u : 0u), results)));
        Pump(conn, () => conn.Events.Count == 2);
        Assert.Null(conn.Events[^1].Next);
        Assert.Equal(1u, conn.Db.Keyed.Iter().Single().Id);
        Assert.Equal(1, handle.Applied);
    }

    [Fact]
    public void ReconnectAttemptsContinuePastTenExceptWhenSessionIsBusy()
    {
        using var conn = Create(options: new() { MinDelay = TimeSpan.FromSeconds(1), MaxDelay = TimeSpan.FromSeconds(5) });
        Establish(conn);
        Drop(conn);
        for (var attempt = 2; attempt <= 10; attempt++)
        {
            Retry(conn);
            Drop(conn);
            Assert.Equal(attempt, conn.Events[^1].Next!.Value.Attempt);
        }

        for (var i = 0; i < 3; i++)
        {
            Retry(conn);
            var count = conn.Events.Count;
            conn.Socket.Lose(SpacetimeDB.WebSocket.CloseError(4000, "session busy"));
            Pump(conn, () => conn.Events.Count > count);
            Assert.Contains("WebSocket closed (4000): session busy", conn.Events[^1].Error!.Message);
            Assert.Equal(10, conn.Events[^1].Next!.Value.Attempt);
            Assert.InRange(conn.Events[^1].Next!.Value.Delay.TotalSeconds, 1, 1.5);
        }

        Retry(conn);
        Drop(conn);
        Assert.Equal(11, conn.Events[^1].Next!.Value.Attempt);
        Retry(conn);
        Establish(conn);
        Drop(conn);
        Assert.Equal(1, conn.Events[^1].Next!.Value.Attempt);
    }

    [Fact]
    public void SessionBusyKeepsAttemptNumberAndFirstDelay()
    {
        using var conn = Create();
        Establish(conn);
        Drop(conn);
        Retry(conn);
        conn.Socket.Lose(new SpacetimeDB.WebSocket.CloseException(4000, "Session busy"));
        Pump(conn, () => conn.Events.Count == 2);
        Assert.Equal(1, conn.Events[^1].Next!.Value.Attempt);
        Assert.InRange(conn.Events[^1].Next!.Value.Delay.TotalMilliseconds, 500, 1500);
    }

    [Fact]
    public void DisconnectWhileWaitingCancelsTheTimer()
    {
        using var conn = Create();
        Establish(conn);
        Drop(conn);
        conn.Disconnect();
        conn.Now += 100;
        conn.FrameTick();
        Assert.Single(conn.Sockets);
        Assert.False(conn.IsReconnecting);
        Assert.Null(conn.Events[^1].Next);
        Assert.Null(conn.Events[^1].Error);
    }

    [Fact]
    public void ReconnectCallbackCanChangeSubscriptionsOrDisconnect()
    {
        using var conn = Create();
        Establish(conn);
        var before = Subscribe(conn);
        Apply(conn, before, Query("keyed", new Row { Id = 1 }));
        Drop(conn);
        ((IDbConnection)conn).AddOnConnect((_, _) =>
        {
            ((IDbConnection)conn).Unsubscribe(before.Id);
            Subscribe(conn, "SELECT * FROM keyed WHERE id = 2");
        });
        Retry(conn);
        Establish(conn);
        Assert.Equal(1, before.Ended);
        Assert.Equal("SELECT * FROM keyed WHERE id = 2", Assert.Single(Batch(conn).Sets).QueryStrings.Single());
        Drop(conn);
        ((IDbConnection)conn).AddOnConnect((_, _) => conn.Disconnect());
        Retry(conn);
        Establish(conn);
        Assert.Empty(conn.Socket.Sent);
        Assert.False(conn.IsReconnecting);
    }

    [Fact]
    public void LostPendingUnsubscribeIsNotReplayed()
    {
        using var conn = Create();
        Establish(conn);
        var handle = Subscribe(conn);
        Apply(conn, handle, Query("keyed", new Row { Id = 1 }));
        ((IDbConnection)conn).Unsubscribe(handle.Id);
        Drop(conn);
        Assert.Equal(1, handle.Ended);
        Retry(conn);
        Establish(conn);
        Assert.Empty(conn.Socket.Sent);
        Assert.Empty(conn.Db.Keyed.Iter());
    }

    [Fact]
    public void InitialSubscriptionsWaitForTheHandshake()
    {
        using var conn = Create();
        var handle = Subscribe(conn);
        Assert.Empty(conn.Socket.Sent);
        Establish(conn);
        Assert.Single(conn.Socket.Sent.OfType<ClientMessage.Subscribe>());
        Apply(conn, handle, Query("keyed", new Row { Id = 1 }));
    }

    [Theory]
    [InlineData(400)]
    [InlineData(401)]
    [InlineData(403)]
    public void AuthenticationRejectionWithoutProviderIsTerminal(int status)
    {
        using var conn = Create();
        Establish(conn);
        Drop(conn);
        Retry(conn);
        conn.Socket.Fail(new SpacetimeDB.WebSocket.ConnectException(status));
        Pump(conn, () => conn.Events.Count == 2);
        Assert.Null(conn.Events[^1].Next);
    }

    [Fact]
    public void BatchWireDiscriminantsMatchTheSharedProtocol()
    {
        ClientMessage client = new ClientMessage.SubscribeBatch(new(0x12345678, new()));
        ServerMessage server = new ServerMessage.SubscribeBatchApplied(new(0x12345678, new()));
        Assert.Equal(new byte[] { 5, 0x78, 0x56, 0x34, 0x12, 0, 0, 0, 0 }, IStructuralReadWrite.ToBytes(new ClientMessage.BSATN(), client));
        Assert.Equal(new byte[] { 8, 0x78, 0x56, 0x34, 0x12, 0, 0, 0, 0 }, IStructuralReadWrite.ToBytes(new ServerMessage.BSATN(), server));
    }

    [Fact]
    public void SubscriptionHandleRebindsItsUnsubscribeId()
    {
        using var conn = Create();
        Establish(conn);
        var handle = new Handle(conn);
        var initial = conn.Socket.Sent.OfType<ClientMessage.Subscribe>().Single().Subscribe_;
        conn.Socket.Receive(new ServerMessage.SubscribeApplied(new(initial.RequestId, initial.QuerySetId, new())));
        Pump(conn, () => handle.IsActive);
        Drop(conn);
        Retry(conn);
        Establish(conn);
        Assert.False(handle.IsActive);
        var replay = Batch(conn);
        var newId = replay.Sets.Single().QuerySetId;
        Assert.NotEqual(initial.QuerySetId.Id, newId.Id);
        conn.Socket.Receive(new ServerMessage.SubscribeBatchApplied(new(replay.RequestId,
            new() { new(newId, new SubscribeSetOutcome.Applied(new QueryRows())) })));
        Pump(conn, () => handle.IsActive);
        handle.Unsubscribe();
        Assert.Equal(newId, conn.Socket.Sent.OfType<ClientMessage.Unsubscribe>().Single().Unsubscribe_.QuerySetId);
    }

    [Fact]
    public void DropDuringReplayKeepsTheOldCacheUntilNextBatch()
    {
        using var conn = Create();
        Establish(conn);
        var handle = Subscribe(conn);
        Apply(conn, handle, Query("keyed", new Row { Id = 1 }));
        Drop(conn);
        Retry(conn);
        Establish(conn);
        var failed = conn.Socket;
        var batch = Batch(conn);
        var id = handle.Id;
        Drop(conn);
        Assert.Equal(1, handle.Applied);
        Assert.Equal(1u, conn.Db.Keyed.Iter().Single().Id);
        Retry(conn);
        Establish(conn);
        failed.Receive(new ServerMessage.SubscribeBatchApplied(new(batch.RequestId,
            new() { new(id, new SubscribeSetOutcome.Applied(Query("keyed", new Row { Id = 999 }))) })));
        conn.Socket.Receive(new ServerMessage.SubscribeBatchApplied(new(Batch(conn).RequestId,
            new() { new(handle.Id, new SubscribeSetOutcome.Applied(Query("keyed", new Row { Id = 2 }))) })));
        Pump(conn, () => handle.Applied == 2);
        Assert.Equal(2u, conn.Db.Keyed.Iter().Single().Id);
    }

    private static string Jwt(double exp, double iat) => "header." + Convert.ToBase64String(Encoding.UTF8.GetBytes(
        System.Text.Json.JsonSerializer.Serialize(new { exp, iat }))).TrimEnd('=').Replace('+', '-').Replace('/', '_') + ".signature";

    [Theory]
    [InlineData(1000, 900, 969, false)]
    [InlineData(1000, 900, 970, true)]
    [InlineData(10000, 0, 9499, false)]
    [InlineData(10000, 0, 9500, true)]
    public void TokenRefreshUsesLifetimeMargin(double exp, double iat, long now, bool refresh)
    {
        Assert.Equal(refresh, ReconnectPolicy.TokenNeedsRefresh(Jwt(exp, iat), DateTimeOffset.FromUnixTimeSeconds(now)));
    }

    [Theory]
    [InlineData(null)]
    [InlineData("garbage")]
    [InlineData("header.e30.signature")]
    public void UnreadableTokenNeedsRefresh(string? token) => Assert.True(ReconnectPolicy.TokenNeedsRefresh(token, DateTimeOffset.UtcNow));

    [Theory]
    [InlineData(1, 0, 1000)]
    [InlineData(1, 0.5, 1000)]
    [InlineData(2, 0.5, 2000)]
    [InlineData(6, 0, 15000)]
    [InlineData(int.MaxValue, 1, 30000)]
    public void BackoffHasJitterAndCap(int attempt, double random, double expected) =>
        Assert.Equal(expected, ReconnectPolicy.Delay(attempt, random, ReconnectPolicy.DefaultMinDelay, ReconnectPolicy.DefaultMaxDelay).TotalMilliseconds);

    private sealed class WarningLogger : ISpacetimeDBLogger
    {
        internal readonly List<string> Warnings = new();
        public void Debug(string message) { }
        public void Trace(string message) { }
        public void Info(string message) { }
        public void Warn(string message) => Warnings.Add(message);
        public void Error(string message) { }
        public void Exception(string message) { }
        public void Exception(Exception e) { }
    }

    [Fact]
    public void CustomBoundsShapeDelaysAndAreFloored()
    {
        var (min, max) = ReconnectPolicy.Resolve(new() { MinDelay = TimeSpan.FromSeconds(2), MaxDelay = TimeSpan.FromSeconds(5) });
        Assert.Equal((TimeSpan.FromSeconds(2), TimeSpan.FromSeconds(5)), (min, max));
        Assert.Equal(2000, ReconnectPolicy.Delay(1, 0.5, min, max).TotalMilliseconds);
        Assert.Equal(4000, ReconnectPolicy.Delay(2, 0.5, min, max).TotalMilliseconds);
        Assert.Equal(5000, ReconnectPolicy.Delay(3, 0.5, min, max).TotalMilliseconds);
        Assert.Equal(2000, ReconnectPolicy.Delay(1, 0, min, max).TotalMilliseconds);

        var previous = Log.Current;
        var logger = new WarningLogger();
        Log.Current = logger;
        try
        {
            Assert.Equal((ReconnectPolicy.MinDelayFloor, ReconnectPolicy.MaxDelayFloor),
                ReconnectPolicy.Resolve(new() { MinDelay = TimeSpan.FromMilliseconds(100), MaxDelay = TimeSpan.FromMilliseconds(200) }));
            Assert.Equal((TimeSpan.FromSeconds(3), TimeSpan.FromSeconds(3)),
                ReconnectPolicy.Resolve(new() { MinDelay = TimeSpan.FromSeconds(3), MaxDelay = TimeSpan.FromSeconds(2) }));
            Assert.Equal(3, logger.Warnings.Count);
        }
        finally
        {
            Log.Current = previous;
        }

        using var conn = Create(options: new() { MinDelay = TimeSpan.FromSeconds(2), MaxDelay = TimeSpan.FromSeconds(5) });
        Establish(conn);
        Drop(conn);
        Assert.InRange(conn.Events[^1].Next!.Value.Delay.TotalMilliseconds, 2000, 3000);
    }
}
