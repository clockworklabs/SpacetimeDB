using System;
using System.Buffers;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Threading;
using System.Threading.Tasks;
#if UNITY_5_3_OR_NEWER
using UnityEngine;
#endif
using SpacetimeDB.BSATN;
using SpacetimeDB.ClientApi;
using Thread = System.Threading.Thread;

namespace SpacetimeDB
{
    public sealed class DbConnectionBuilder<DbConnection>
        where DbConnection : IDbConnection, new()
    {
        readonly DbConnection conn = new();

        string? uri;
        string? nameOrAddress;
        string? token;
        Compression? compression;
        bool light;
        bool? confirmedReads;
        bool automaticReconnect;
        (TimeSpan Min, TimeSpan Max) reconnectDelays = (ReconnectPolicy.DefaultMinDelay, ReconnectPolicy.DefaultMaxDelay);
        Func<Task<string>>? tokenProvider;

        public DbConnection Build()
        {
            if (uri == null)
            {
                throw new InvalidOperationException("Building DbConnection with a null uri. Call WithUri() first.");
            }
            if (nameOrAddress == null)
            {
                throw new InvalidOperationException("Building DbConnection with a null nameOrAddress. Call WithDatabaseName() first.");
            }
            conn.ConfigureReconnect(automaticReconnect, tokenProvider, reconnectDelays.Min, reconnectDelays.Max);
            conn.Connect(token, uri, nameOrAddress, compression ?? Compression.Brotli, light, confirmedReads);
#if UNITY_5_3_OR_NEWER
            if (SpacetimeDBNetworkManager._instance != null)
            {
                SpacetimeDBNetworkManager._instance.AddConnection(conn);
            }
#endif
            return conn;
        }

        public DbConnectionBuilder<DbConnection> WithUri(string uri)
        {
            this.uri = uri;
            return this;
        }

        public DbConnectionBuilder<DbConnection> WithDatabaseName(string nameOrAddress)
        {
            this.nameOrAddress = nameOrAddress;
            return this;
        }

        public DbConnectionBuilder<DbConnection> WithToken(string? token)
        {
            this.token = token;
            return this;
        }

        /// <summary>
        /// Reconnect after an established connection drops, preserving handles and callbacks.
        /// Retries use exponential backoff from <c>MinDelay</c> (default 1 s) up to <c>MaxDelay</c>
        /// (default 30 s). Values below the 500 ms and 1 s floors are raised with a warning, so
        /// that retrying clients cannot overwhelm the database.
        /// </summary>
        public DbConnectionBuilder<DbConnection> WithAutomaticReconnect(AutomaticReconnectOptions? options = null)
        {
            automaticReconnect = true;
            if (options != null) reconnectDelays = ReconnectPolicy.Resolve(options);
            return this;
        }

        public DbConnectionBuilder<DbConnection> WithTokenProvider(Func<Task<string>> provider)
        {
            tokenProvider = provider ?? throw new ArgumentNullException(nameof(provider));
            return this;
        }

        public DbConnectionBuilder<DbConnection> WithCompression(Compression compression)
        {
            this.compression = compression;
            return this;
        }

        public DbConnectionBuilder<DbConnection> WithLightMode(bool light)
        {
            this.light = light;
            return this;
        }

        public DbConnectionBuilder<DbConnection> WithConfirmedReads(bool confirmedReads)
        {
            this.confirmedReads = confirmedReads;
            return this;
        }

        public delegate void ConnectCallback(DbConnection conn, Identity identity, string token);

        public DbConnectionBuilder<DbConnection> OnConnect(ConnectCallback cb)
        {
            conn.AddOnConnect((identity, token) => cb(conn, identity, token));
            return this;
        }

        public delegate void ConnectErrorCallback(Exception e);
        public delegate void ConnectErrorWithReconnectCallback(Exception e, NextReconnect? nextReconnect);

        public DbConnectionBuilder<DbConnection> OnConnectError(ConnectErrorCallback cb) =>
            OnConnectError((e, _) => cb(e));

        public DbConnectionBuilder<DbConnection> OnConnectError(ConnectErrorWithReconnectCallback cb)
        {
            conn.AddOnConnectError((e, next) => cb(e, next));
            return this;
        }

        public delegate void DisconnectCallback(DbConnection conn, Exception? e);
        public delegate void DisconnectWithReconnectCallback(DbConnection conn, Exception? e, NextReconnect? nextReconnect);

        public DbConnectionBuilder<DbConnection> OnDisconnect(DisconnectCallback cb) =>
            OnDisconnect((conn, e, _) => cb(conn, e));

        public DbConnectionBuilder<DbConnection> OnDisconnect(DisconnectWithReconnectCallback cb)
        {
            conn.AddOnDisconnect((e, next) => cb(conn, e, next));
            return this;
        }
    }

    public interface IDbConnection
    {
        internal void Connect(string? token, string uri, string addressOrName, Compression compression, bool light, bool? confirmedReads);

        internal void AddOnConnect(Action<Identity, string> cb);
        internal void AddOnConnectError(Action<Exception, NextReconnect?> cb);
        internal void AddOnDisconnect(Action<Exception?, NextReconnect?> cb);
        internal void ConfigureReconnect(bool enabled, Func<Task<string>>? provider, TimeSpan minDelay, TimeSpan maxDelay);
        bool IsReconnecting { get; }
        internal bool AutomaticReconnectEnabled { get; }

        internal QuerySetId? Subscribe(ISubscriptionHandle handle, string[] querySqls);
        internal void Unsubscribe(QuerySetId queryId);
        void FrameTick();
        void Disconnect();

        internal Task<T[]> RemoteQuery<T>(string query) where T : IStructuralReadWrite, new();
        void InternalCallReducer<T>(T args)
            where T : IReducerArgs, new();

        void InternalCallProcedure<TArgs, TReturn>(
            TArgs args,
            ProcedureCallback<TReturn> callback)
            where TArgs : IProcedureArgs, new()
            where TReturn : IStructuralReadWrite, new();
    }

    public abstract partial class DbConnectionBase<DbConnection, Tables, Reducer> : IDbConnection
        where DbConnection : DbConnectionBase<DbConnection, Tables, Reducer>, new()
        where Tables : RemoteTablesBase
    {
        /// <remarks>
        /// This isn't reset since [RuntimeInitializeOnLoadMethod] methods cannot be in generic types
        /// We assume that the user will reset this if needed; Unity will give an error about this field not being reset.
        /// One way we can get around this in the future is using <see href="https://docs.unity3d.com/6000.5/Documentation/ScriptReference/Unity.Scripting.LifecycleManagement.AutoStaticsCleanupAttribute.html">AutoStaticsCleanup</see>
        /// But that requires Unity 6.5
        /// </remarks>
        internal static bool IsTesting { get; set; } = false;

        public static DbConnectionBuilder<DbConnection> Builder() => new();

        internal event Action<Identity, string>? onConnect;

        /// <summary>
        /// Called when an exception occurs when sending a message.
        /// </summary>
        [Obsolete]
        public event Action<Exception>? onSendError;

        /// <summary>
        /// Dictionary of subscriptions, keyed by query ID.
        /// </summary>
        private readonly Dictionary<uint, ISubscriptionHandle> subscriptions = new();

        /// <summary>
        /// Allocates query IDs.
        /// </summary>
        private UintAllocator querySetIdAllocator;

        public ConnectionId ConnectionId { get; private set; } = ConnectionId.Random();
        public Identity? Identity { get; private set; }
        private ConnectionId? initialConnectionId;
        private bool onConnectInvoked;

        internal WebSocket webSocket;
        private bool connectionClosed;
        public abstract Tables Db { get; }

        protected abstract IEventContext ToEventContext(Event<Reducer> Event);
        protected abstract IReducerEventContext ToReducerEventContext(ReducerEvent<Reducer> reducerEvent);
        protected abstract ISubscriptionEventContext MakeSubscriptionEventContext();
        protected abstract IErrorContext ToErrorContext(Exception errorContext);
        protected abstract IProcedureEventContext ToProcedureEventContext(ProcedureEvent procedureEvent);

        private readonly ConcurrentDictionary<uint, TaskCompletionSource<OneOffQueryResult>> waitingOneOffQueries = new();

        private readonly ConcurrentDictionary<uint, PendingReducerCall> pendingReducerCalls = new();

        private sealed class PendingReducerCall
        {
            public Reducer Reducer = default!;
        }

        private readonly ProcedureCallbacks procedureCallbacks = new();

        private void FailPendingOperations(Exception error)
        {
            stats.ClearRequestsAwaitingResponse();

            foreach (var (requestId, _) in waitingOneOffQueries.ToArray())
            {
                if (waitingOneOffQueries.TryRemove(requestId, out var resultSource))
                {
                    resultSource.TrySetException(error);
                }
            }

            foreach (var entry in pendingReducerCalls.ToArray())
            {
                if (!pendingReducerCalls.TryRemove(entry.Key, out var pending) || !automaticReconnect) continue;
                try
                {
                    var reducerEvent = new ReducerEvent<Reducer>(default, new Status.UnknownResult(default),
                        Identity ?? default, ConnectionId, null, pending.Reducer);
                    Dispatch(ToReducerEventContext(reducerEvent), pending.Reducer);
                }
                catch (Exception e) { Log.Exception(e); }
            }

            try
            {
                var procedureEvent = new ProcedureEvent(
                    default,
                    new ProcedureStatus.InternalError(error.Message),
                    Identity ?? default,
                    ConnectionId,
                    default,
                    0
                );
                var ctx = ToProcedureEventContext(procedureEvent);
                procedureCallbacks.FailAll(ctx, error);
            }
            catch
            {
                // If we cannot construct a procedure context, still avoid retaining stale callbacks.
                procedureCallbacks.Clear();
            }
        }

        private volatile bool isClosing;
#if !(UNITY_WEBGL && !UNITY_EDITOR)
        private readonly Thread networkMessageParseThread;
#endif
        public readonly Stats stats = new();

        protected DbConnectionBase()
        {
            webSocket = CreateWebSocket();
#if UNITY_WEBGL && !UNITY_EDITOR
            if (SpacetimeDBNetworkManager._instance != null)
                SpacetimeDBNetworkManager._instance.StartCoroutine(ParseMessages());
#endif

#if !(UNITY_WEBGL && !UNITY_EDITOR)
            // For targets other than webgl we start a thread to parse messages
            networkMessageParseThread = new Thread(ParseMessages);
            networkMessageParseThread.Name = "SpacetimeDB Network Thread";
            networkMessageParseThread.Start();
#endif
        }

        internal struct UnparsedMessage
        {
            /// <summary>
            /// The bytes of the message.
            /// </summary>
            public byte[] bytes;
            public int generation;
            public Action? action;

            /// <summary>
            /// The timestamp the message came off the wire.
            /// </summary>
            public DateTime timestamp;

            /// <summary>
            /// The ID assigned by the message parsing queue tracker.
            /// </summary>
            public uint parseQueueTrackerId;
        }

        internal struct ParsedMessage
        {
            public ServerMessage message;
            public int generation;
            public Action? action;
            public Exception? error;
            public Status? reducerStatus;
            public ParsedDatabaseUpdate dbOps;
            public DateTime receiveTimestamp;
            public uint applyQueueTrackerId;
        }

        private readonly BlockingCollection<UnparsedMessage> _parseQueue =
            new(new ConcurrentQueue<UnparsedMessage>());

        private readonly BlockingCollection<ParsedMessage> _applyQueue =
            new(new ConcurrentQueue<ParsedMessage>());

        internal bool HasMessageToApply => _applyQueue.Count > 0;

        private readonly CancellationTokenSource _parseCancellationTokenSource = new();
        private CancellationToken _parseCancellationToken => _parseCancellationTokenSource.Token;

        private static readonly Status Committed = new Status.Committed(default);

        /// <summary>
        /// Get a description of a message suitable for storing in the tracker metadata.
        /// </summary>
        /// <param name="message"></param>
        /// <returns></returns>
        internal string TrackerMetadataForMessage(ServerMessage message) => message switch
        {
            ServerMessage.TransactionUpdate(var transactionUpdate) => $"type={nameof(ServerMessage.TransactionUpdate)},query_sets={transactionUpdate.QuerySets.Count}",
            ServerMessage.ReducerResult(var reducerResult) => $"type={nameof(ServerMessage.ReducerResult)},request_id={reducerResult.RequestId}",
            _ => $"type={message.GetType().Name}"
        };

#if UNITY_WEBGL && !UNITY_EDITOR
        internal System.Collections.IEnumerator ParseMessages()
#else
        internal void ParseMessages()
#endif
        {
            static BsatnRowList EmptyRowList() => new(new RowSizeHint.RowOffsets(new()), new());

            IEnumerable<(IRemoteTableHandle, TableUpdate)> GetTables(IEnumerable<TableUpdate> updates)
            {
                foreach (var update in updates)
                {
                    var tableName = update.TableName;
                    var table = Db.GetTable(tableName);
                    if (table == null)
                    {
                        Log.Error($"Unknown table name: {tableName}");
                        continue;
                    }
                    yield return (table, update);
                }
            }

            ParsedDatabaseUpdate ParseSubscribeRows(QueryRows queryRows, ParsedDatabaseUpdate? target = null)
            {
                var dbOps = target ?? ParsedDatabaseUpdate.New();
                var empty = EmptyRowList();
                foreach (var tableRows in queryRows.Tables)
                {
                    var table = Db.GetTable(tableRows.Table);
                    if (table == null)
                    {
                        Log.Error($"Unknown table name: {tableRows.Table}");
                        continue;
                    }

                    var update = new TableUpdate
                    {
                        TableName = tableRows.Table,
                        Rows = new List<TableUpdateRows>
                        {
                            new TableUpdateRows.PersistentTable(
                                new PersistentTableRows(tableRows.Rows, empty)
                            )
                        }
                    };
                    table.ParseInsertOnly(update, dbOps);
                }
                return dbOps;
            }

            ParsedDatabaseUpdate ParseUnsubscribeRows(QueryRows queryRows)
            {
                var dbOps = ParsedDatabaseUpdate.New();
                var empty = EmptyRowList();
                foreach (var tableRows in queryRows.Tables)
                {
                    var table = Db.GetTable(tableRows.Table);
                    if (table == null)
                    {
                        Log.Error($"Unknown table name: {tableRows.Table}");
                        continue;
                    }

                    var update = new TableUpdate
                    {
                        TableName = tableRows.Table,
                        Rows = new List<TableUpdateRows>
                        {
                            new TableUpdateRows.PersistentTable(
                                new PersistentTableRows(empty, tableRows.Rows)
                            )
                        }
                    };
                    table.ParseDeleteOnly(update, dbOps);
                }
                return dbOps;
            }

            ParsedDatabaseUpdate ParseTransactionUpdate(TransactionUpdate update)
            {
                var dbOps = ParsedDatabaseUpdate.New();
                foreach (var set in update.QuerySets)
                {
                    foreach (var (table, tableUpdate) in GetTables(set.Tables))
                    {
                        table.Parse(tableUpdate, dbOps);
                    }
                }
                return dbOps;
            }

            string DecodeReducerError(List<byte> bytes)
            {
                try
                {
                    using var stream = BSATNHelpers.MakePooledListStream(bytes, out var pooledBuffer);
                    try
                    {
                        using var reader = new BinaryReader(stream);
                        return new SpacetimeDB.BSATN.String().Read(reader);
                    }
                    finally
                    {
                        ArrayPool<byte>.Shared.Return(pooledBuffer);
                    }
                }
                catch
                {
                    return $"Reducer returned undecodable BSATN string bytes (len={bytes.Count})";
                }
            }

            while (!isClosing)
            {

#if UNITY_WEBGL && !UNITY_EDITOR
                yield return null;
                while (_parseQueue.Count > 0)
#endif
                try
                {
                    var message = _parseQueue.Take(_parseCancellationToken);
                    if (message.generation != socketGeneration) continue;
                    ParsedMessage parsedMessage;
                    try
                    {
                        parsedMessage = message.action != null
                            ? new ParsedMessage { action = message.action, generation = message.generation }
                            : ParseMessage(message);
                    }
                    catch (Exception e)
                    {
                        parsedMessage = new ParsedMessage { error = e, generation = message.generation };
                    }
                    _applyQueue.Add(parsedMessage, _parseCancellationToken);
                }
                catch (OperationCanceledException)
                {
#if UNITY_WEBGL && !UNITY_EDITOR
                    break;
#else
                    return; // Normal shutdown
#endif
                }
                catch (Exception e)
                {
                    Log.Exception(e);
                    _applyQueue.Add(new ParsedMessage { error = e, generation = socketGeneration });
#if UNITY_WEBGL && !UNITY_EDITOR
                    break;
#else
                    return;
#endif
                }
            }

            ParsedMessage ParseMessage(UnparsedMessage unparsed)
            {
                var dbOps = ParsedDatabaseUpdate.New();
                var message = CompressionHelpers.DecompressDecodeMessage(unparsed.bytes);
                var trackerMetadata = TrackerMetadataForMessage(message);

                stats.ParseMessageQueueTracker.FinishTrackingRequest(unparsed.parseQueueTrackerId, trackerMetadata);
                var parseStart = DateTime.UtcNow;

                Status? reducerStatus = null;

                switch (message)
                {
                    case ServerMessage.InitialConnection:
                        break;
                    case ServerMessage.SubscribeApplied(var subscribeApplied):
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(subscribeApplied.RequestId, unparsed.timestamp);
                        dbOps = ParseSubscribeRows(subscribeApplied.Rows);
                        break;
                    case ServerMessage.SubscribeBatchApplied(var batch):
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(batch.RequestId, unparsed.timestamp);
                        foreach (var result in batch.Results)
                        {
                            if (result.Outcome is SubscribeSetOutcome.Applied(var rows))
                                ParseSubscribeRows(rows, dbOps);
                        }
                        break;
                    case ServerMessage.UnsubscribeApplied(var unsubscribeApplied):
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(unsubscribeApplied.RequestId, unparsed.timestamp);
                        if (unsubscribeApplied.Rows != null)
                        {
                            dbOps = ParseUnsubscribeRows(unsubscribeApplied.Rows);
                        }
                        break;
                    case ServerMessage.SubscriptionError(var subscriptionError):
                        if (subscriptionError.RequestId.HasValue)
                        {
                            stats.SubscriptionRequestTracker.FinishTrackingRequest(subscriptionError.RequestId.Value, unparsed.timestamp);
                        }
                        break;
                    case ServerMessage.TransactionUpdate(var transactionUpdate):
                        dbOps = ParseTransactionUpdate(transactionUpdate);
                        break;
                    case ServerMessage.OneOffQueryResult(var resp):
                        // Queries do not mutate the client cache and must complete without FrameTick.
                        // Recheck after decoding in case the socket was replaced while parsing.
                        if (!isClosing && unparsed.generation == socketGeneration &&
                            waitingOneOffQueries.TryRemove(resp.RequestId, out var completion))
                            completion.TrySetResult(resp);
                        break;
                    case ServerMessage.ReducerResult(var reducerResult):
                        if (!stats.ReducerRequestTracker.FinishTrackingRequest(reducerResult.RequestId, unparsed.timestamp))
                        {
                            Log.Warn($"Failed to finish tracking reducer request: {reducerResult.RequestId}");
                        }

                        reducerStatus = reducerResult.Result switch
                        {
                            ReducerOutcome.Ok => Committed,
                            ReducerOutcome.OkEmpty => Committed,
                            ReducerOutcome.Err(var err) => new Status.Failed(DecodeReducerError(err)),
                            ReducerOutcome.InternalError(var err) => new Status.Failed(err),
                            _ => new Status.Failed("Unknown reducer result"),
                        };

                        if (reducerResult.Result is ReducerOutcome.Ok(var ok))
                        {
                            dbOps = ParseTransactionUpdate(ok.TransactionUpdate);
                        }

                        break;
                    case ServerMessage.ProcedureResult(var procedureResult):
                        if (!stats.ProcedureRequestTracker.FinishTrackingRequest(procedureResult.RequestId, unparsed.timestamp))
                        {
                            Log.Warn($"Failed to finish tracking procedure request: {procedureResult.RequestId}");
                        }

                        break;
                    default:
                        throw new InvalidOperationException();
                }

                stats.ParseMessageTracker.InsertRequest(parseStart, trackerMetadata);
                var applyTracker = stats.ApplyMessageQueueTracker.StartTrackingRequest(trackerMetadata);

                return new ParsedMessage { generation = unparsed.generation, reducerStatus = reducerStatus, message = message, dbOps = dbOps, receiveTimestamp = unparsed.timestamp, applyQueueTrackerId = applyTracker };
            }
        }

        public void Disconnect()
        {
            if (isClosing) return;
            EndConnection();
            onDisconnect?.Invoke(null, null);
        }

        /// <summary>
        /// Connect to a remote spacetime instance.
        /// </summary>
        /// <param name="uri"> URI of the SpacetimeDB server (ex: https://maincloud.spacetimedb.com)
        /// <param name="addressOrName">The name or address of the database to connect to</param>
        /// <param name="compression">The compression settings to use</param>
        /// <param name="light">Whether or not to request light updates</param>
        /// <param name="confirmedReads">
        /// If set to true, instruct the server to send updates for transactions
        /// only after they are confirmed to be durable.
        ///
        /// What durable means depends on the server configuration. In general,
        /// a transaction is durable when it has been written to disk on one or
        /// more servers.
        ///
        /// If set to false, instruct the server to send updates as soon as
        /// transactions are committed in memory.
        ///
        /// If not set, the server chooses the default.
        /// </param>
        void IDbConnection.Connect(string? token, string uri, string addressOrName, Compression compression, bool light, bool? confirmedReads)
        {
            retainedToken = token;
            uri = uri.Replace("http://", "ws://");
            uri = uri.Replace("https://", "wss://");
            if (!uri.StartsWith("ws://") && !uri.StartsWith("wss://"))
            {
                uri = $"ws://{uri}";
            }
            // Things fail surprisingly if we have a trailing slash, because we later manually append strings
            // like `/foo` and then end up with `//` in the URI.
            uri = uri.TrimEnd('/');

            connectionOptions = (uri, addressOrName, compression, light, confirmedReads);
            preparingReplay = automaticReconnect;
            StartSocket();
        }


        private void ApplyUpdate(IEventContext eventContext, ParsedDatabaseUpdate dbOps)
        {
            // First trigger OnBeforeDelete
            foreach (var (table, update) in dbOps.Updates)
            {
                table.PreApply(eventContext, update);
            }

            foreach (var (table, update) in dbOps.Updates)
            {
                table.Apply(eventContext, update);
            }

            foreach (var (table, _) in dbOps.Updates)
            {
                table.PostApply(eventContext);
            }
        }

        protected abstract bool Dispatch(IReducerEventContext context, Reducer reducer);

        private void ApplyMessage(ParsedMessage parsed)
        {
            if (parsed.generation != socketGeneration || isClosing) return;
            if (parsed.action != null)
            {
                parsed.action();
                return;
            }
            if (parsed.error != null)
            {
                HandleSocketFailure(new ConnectionProtocolException("Could not parse server message.", parsed.error));
                return;
            }
            if (automaticReconnect && !onConnectInvoked && parsed.message is not ServerMessage.InitialConnection)
            {
                HandleSocketFailure(new ConnectionProtocolException("Expected InitialConnection."));
                return;
            }
            var message = parsed.message;
            var dbOps = parsed.dbOps;
            var timestamp = parsed.receiveTimestamp;

            stats.ApplyMessageQueueTracker.FinishTrackingRequest(parsed.applyQueueTrackerId);
            var applyStart = DateTime.UtcNow;

            switch (message)
            {
                case ServerMessage.SubscribeBatchApplied(var batch):
                    ApplyReplayBatch(batch, dbOps);
                    break;
                case ServerMessage.SubscribeApplied(var subscribeApplied):
                    {
                        var eventContext = MakeSubscriptionEventContext();
                        var legacyEventContext = ToEventContext(new Event<Reducer>.SubscribeApplied());
                        ApplyUpdate(legacyEventContext, dbOps);
                        if (subscriptions.TryGetValue(subscribeApplied.QuerySetId.Id, out var subscription))
                        {
                            try
                            {
                                subscription.OnApplied(eventContext);
                            }
                            catch (Exception e)
                            {
                                Log.Exception(e);
                            }
                        }
                        else
                        {
                            Log.Warn($"Received SubscribeApplied for unknown query_set_id={subscribeApplied.QuerySetId.Id}");
                        }

                        break;
                    }

                case ServerMessage.SubscriptionError(var subscriptionError):
                    {
                        Log.Warn($"Subscription Error: ${subscriptionError.Error}");

                        // TODO: should I use a more specific exception type here?
                        var exception = new Exception(subscriptionError.Error);
                        var eventContext = ToErrorContext(exception);
                        var legacyEventContext = ToEventContext(new Event<Reducer>.SubscribeError(exception));
                        ApplyUpdate(legacyEventContext, dbOps);
                        if (subscriptions.TryGetValue(subscriptionError.QuerySetId.Id, out var subscription))
                        {
                            try
                            {
                                subscription.OnError(eventContext);
                            }
                            catch (Exception e)
                            {
                                Log.Exception(e);
                            }

                            RemoveSubscription(subscriptionError.QuerySetId.Id);
                        }
                        else
                        {
                            Log.Warn($"Received SubscriptionError for unknown query_set_id={subscriptionError.QuerySetId.Id}");
                        }

                        break;
                    }

                case ServerMessage.UnsubscribeApplied(var unsubscribeApplied):
                    {
                        var eventContext = MakeSubscriptionEventContext();
                        var legacyEventContext = ToEventContext(new Event<Reducer>.UnsubscribeApplied());
                        ApplyUpdate(legacyEventContext, dbOps);
                        if (subscriptions.TryGetValue(unsubscribeApplied.QuerySetId.Id, out var subscription))
                        {
                            try
                            {
                                subscription.OnEnded(eventContext);
                            }
                            catch (Exception e)
                            {
                                Log.Exception(e);
                            }
                        }

                        RemoveSubscription(unsubscribeApplied.QuerySetId.Id);
                    }
                    break;

                case ServerMessage.TransactionUpdate(var transactionUpdate):
                    {
                        var legacyEventContext = ToEventContext(new Event<Reducer>.Transaction());
                        ApplyUpdate(legacyEventContext, dbOps);
                        break;
                    }
                case ServerMessage.ReducerResult(var reducerResult):
                    {
                        if (pendingReducerCalls.TryRemove(reducerResult.RequestId, out var pending))
                        {
                            var reducerEvent = new ReducerEvent<Reducer>(
                                (DateTimeOffset)reducerResult.Timestamp, parsed.reducerStatus!,
                                Identity ?? default, ConnectionId, null, pending.Reducer);
                            var legacyEventContext = ToEventContext(new Event<Reducer>.Reducer(reducerEvent));
                            ApplyUpdate(legacyEventContext, dbOps);
                            var eventContext = ToReducerEventContext(reducerEvent);
                            Dispatch(eventContext, reducerEvent.Reducer);
                        }
                        else
                        {
                            HandleSocketFailure(new ConnectionProtocolException($"Reducer result for unknown request_id {reducerResult.RequestId}."));
                        }
                        break;
                    }
                case ServerMessage.InitialConnection(var initialConnection):
                    HandleInitialConnection(initialConnection);
                    break;

                case ServerMessage.OneOffQueryResult:
                    // Completed by the parser independently of FrameTick.
                    break;
                case ServerMessage.ProcedureResult(var procedureResult):
                    var procedureEventContext = ToProcedureEventContext(new ProcedureEvent(
                        procedureResult.Timestamp, procedureResult.Status, Identity ?? default,
                        ConnectionId, procedureResult.TotalHostExecutionDuration, procedureResult.RequestId));
                    if (!procedureCallbacks.TryResolveCallback(procedureEventContext, procedureResult.RequestId, procedureResult))
                    {
                        Log.Warn($"Received ProcedureResult for unknown request ID: {procedureResult.RequestId}");
                    }
                    break;
                default:
                    throw new InvalidOperationException();
            }

            stats.ApplyMessageTracker.InsertRequest(applyStart, TrackerMetadataForMessage(message));
        }

        // Note: this method is called from unit tests.
        internal void OnMessageReceived(byte[] bytes, DateTime timestamp)
        {
            EnqueueMessage(bytes, timestamp, socketGeneration);
        }

        void IDbConnection.InternalCallReducer<T>(T args)
        {
            if (automaticReconnect && !IsActive) throw new InvalidOperationException("Not connected to server.");
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot call reducer, not connected to server!");
                return;
            }

            var requestId = stats.ReducerRequestTracker.StartTrackingRequest(args.ReducerName);
            if (args is not Reducer typedReducer)
            {
                throw new InvalidOperationException(
                    $"Reducer arguments type {typeof(T).FullName} is not assignable to {typeof(Reducer).FullName}."
                );
            }

            var encodedArgs = IStructuralReadWrite.ToBytes(args).ToList();
            var pendingReducer = new PendingReducerCall
            {
                Reducer = typedReducer,
            };
            pendingReducerCalls[requestId] = pendingReducer;
            webSocket.Send(new ClientMessage.CallReducer(new CallReducer(
                requestId,
                0, // v2 parity with Rust SDK: always CallReducerFlags::Default.
                args.ReducerName,
                encodedArgs
            )));
        }

        // TODO: Replace with an internal interface 
        void IDbConnection.InternalCallProcedure<TArgs, TReturn>(
            TArgs args,
            ProcedureCallback<TReturn> callback)
        {
            if (automaticReconnect && !IsActive) throw new InvalidOperationException("Not connected to server.");
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot call procedure, not connected to server!");
                return;
            }

            var requestId = stats.ProcedureRequestTracker.StartTrackingRequest(args.ProcedureName);
            procedureCallbacks.RegisterCallback(requestId, callback);

            webSocket.Send(new ClientMessage.CallProcedure(new CallProcedure(
                requestId,
                0,
                args.ProcedureName,
                IStructuralReadWrite.ToBytes(args).ToList()
            )));
        }

        QuerySetId? IDbConnection.Subscribe(ISubscriptionHandle handle, string[] querySqls)
        {
            if (!automaticReconnect && !webSocket.IsConnected)
            {
                Log.Error("Cannot subscribe, not connected to server!");
                return null;
            }
            if (isClosing) throw new InvalidOperationException("Connection closed.");
            var querySetId = querySetIdAllocator.Next();
            subscriptions[querySetId] = handle;
            subscriptionQueries[querySetId] = (string[])querySqls.Clone();
            if (!automaticReconnect || (IsActive && !preparingReplay))
                SendSubscription(querySetId);
            return new QuerySetId(querySetId);
        }

        /// Usage: SpacetimeDBClientBase.instance.OneOffQuery<Message>("SELECT * FROM table WHERE sender = \"bob\"");
        [Obsolete("This is replaced by ctx.Db.TableName.RemoteQuery(\"WHERE ...\")", false)]
        public Task<T[]> OneOffQuery<T>(string query) where T : IStructuralReadWrite, new() =>
            ((IDbConnection)this).RemoteQuery<T>(query);

        async Task<T[]> IDbConnection.RemoteQuery<T>(string query)
        {
            if (!IsActive)
            {
                var error = "Cannot run one-off query, not connected to server!";
                Log.Error(error);
                throw new InvalidOperationException(error);
            }

            var requestId = stats.OneOffRequestTracker.StartTrackingRequest();
            var resultSource = new TaskCompletionSource<OneOffQueryResult>(TaskCreationOptions.RunContinuationsAsynchronously);
            waitingOneOffQueries[requestId] = resultSource;

            try
            {
                webSocket.Send(new ClientMessage.OneOffQuery(new OneOffQuery
                {
                    RequestId = requestId,
                    QueryString = query,
                }));
            }
            catch
            {
                waitingOneOffQueries.TryRemove(requestId, out _);
                stats.OneOffRequestTracker.RemoveRequestAwaitingResponse(requestId);
                throw;
            }

            // Keep row decoding off the caller's synchronization context (e.g. Unity's main thread).
            var result = await resultSource.Task.ConfigureAwait(false);

            if (!stats.OneOffRequestTracker.FinishTrackingRequest(requestId))
            {
                Log.Warn($"Failed to finish tracking one off request: {requestId}");
            }

            T[] LogAndThrow(string error)
            {
                error = $"While processing one-off-query `{query}`, request_id {requestId}: {error}";
                Log.Error(error);
                throw new Exception(error);
            }

            if (result.Result is Result<QueryRows, string>.ErrR(var err))
            {
                return LogAndThrow($"Server error: {err}");
            }

            if (result.Result is not Result<QueryRows, string>.OkR(var rows))
            {
                return LogAndThrow("Unexpected one-off query result variant");
            }

            var tables = rows.Tables;
            if (tables.Count != 1)
            {
                return LogAndThrow($"Expected a single table, but got {tables.Count}");
            }

            var resultTable = tables[0];
            var cacheTable = Db.GetTable(resultTable.Table);

            if (cacheTable?.ClientTableType != typeof(T))
            {
                return LogAndThrow($"Mismatched result type, expected {typeof(T)} but got {resultTable.Table}");
            }

            var (resultReader, resultCount) = CompressionHelpers.ParseRowList(resultTable.Rows);
            var output = new T[resultCount];
            for (int i = 0; i < resultCount; i++)
            {
                output[i] = IStructuralReadWrite.Read<T>(resultReader);
            }
            return output;
        }

        public bool IsActive => !connectionClosed && webSocket.IsConnected && (!automaticReconnect || onConnectInvoked);

        public void FrameTick()
        {
            webSocket.Update();
            while (_applyQueue.TryTake(out var parsedMessage))
            {
                ApplyMessage(parsedMessage);
            }
            TickReconnect();
        }

        void IDbConnection.Unsubscribe(QuerySetId queryId)
        {
            if (!subscriptions.ContainsKey(queryId.Id))
            {
                Log.Warn($"Unsubscribing from a subscription that the DbConnection does not know about, with QuerySetId {queryId.Id}");
            }

            unsubscribeRequested.Add(queryId.Id);
            if (automaticReconnect && (!IsActive || preparingReplay))
            {
                EndSubscription(queryId.Id);
                return;
            }
            var requestId = stats.SubscriptionRequestTracker.StartTrackingRequest();

            webSocket.Send(new ClientMessage.Unsubscribe(new()
            {
                RequestId = requestId,
                QuerySetId = queryId,
                Flags = UnsubscribeFlags.SendDroppedRows,
            }));

        }

        void IDbConnection.AddOnConnect(Action<Identity, string> cb) => onConnect += cb;

        void IDbConnection.AddOnConnectError(Action<Exception, NextReconnect?> cb) => onConnectError += cb;

        void IDbConnection.AddOnDisconnect(Action<Exception?, NextReconnect?> cb) => onDisconnect += cb;
    }

    /// <summary>
    /// Represents the result of parsing a database update message from SpacetimeDB.
    /// Contains updates for all tables affected by the update, with each entry mapping a table handle
    /// to its respective set of row changes (by primary key or row instance).
    ///
    /// Note: Due to C#'s struct constructor limitations, you must use <see cref="ParsedDatabaseUpdate.New"/>
    /// to create new instances.
    /// Do not use the default constructor, as it will not initialize the Updates dictionary.
    /// </summary>
    internal struct ParsedDatabaseUpdate
    {
        // Map: table handles -> (primary key -> IStructuralReadWrite).
        // If a particular table has no primary key, the "primary key" is just the row itself.
        // This is valid because any [SpacetimeDB.Type] automatically has a correct Equals and HashSet implementation.
        public Dictionary<IRemoteTableHandle, IParsedTableUpdate> Updates;

        // Can't override the default constructor. Make sure you use this one!
        public static ParsedDatabaseUpdate New()
        {
            ParsedDatabaseUpdate result;
            result.Updates = new();
            return result;
        }

        /// <summary>
        /// Returns the <see cref="IParsedTableUpdate"/> for the specified table.
        /// If no update exists for the table, a new one is allocated and added to the Updates dictionary.
        /// </summary>
        public IParsedTableUpdate UpdateForTable(IRemoteTableHandle table)
        {
            if (!Updates.TryGetValue(table, out var delta))
            {
                delta = table.MakeParsedTableUpdate();
                Updates[table] = delta;
            }

            return delta;
        }
    }

    internal struct UintAllocator
    {
        private uint lastAllocated;

        /// <summary>
        /// Allocate a new ID in a thread-unsafe way.
        /// </summary>
        /// <returns>A previously-unused ID.</returns>
        public uint Next()
        {
            lastAllocated++;
            return lastAllocated;
        }
    }
}
