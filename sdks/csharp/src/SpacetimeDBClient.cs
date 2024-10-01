using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Net.WebSockets;
using System.Threading;
using System.Threading.Tasks;
using SpacetimeDB.BSATN;
using SpacetimeDB.Internal;
using SpacetimeDB.ClientApi;
using Thread = System.Threading.Thread;
using System.Runtime.CompilerServices;

[assembly: InternalsVisibleTo("SpacetimeDB.Tests")]

namespace SpacetimeDB
{
    public sealed class DbConnectionBuilder<DbConnection, Reducer>
        where DbConnection : DbConnectionBase<DbConnection, Reducer>, new()
    {
        readonly DbConnection conn = new();

        string? uri;
        string? nameOrAddress;
        string? token;

        public DbConnection Build()
        {
            if (uri == null)
            {
                throw new InvalidOperationException("Building DbConnection with a null uri. Call WithUri() first.");
            }
            if (nameOrAddress == null)
            {
                throw new InvalidOperationException("Building DbConnection with a null nameOrAddress. Call WithModuleName() first.");
            }
            conn.Connect(token, uri, nameOrAddress);
            return conn;
        }

        public DbConnectionBuilder<DbConnection, Reducer> WithUri(string uri)
        {
            this.uri = uri;
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> WithModuleName(string nameOrAddress)
        {
            this.nameOrAddress = nameOrAddress;
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> WithCredentials(in (Identity identity, string token)? creds)
        {
            token = creds?.token;
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> OnConnect(Action<Identity, string> cb)
        {
            conn.onConnect += cb;
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> OnConnectError(Action<WebSocketError?, string> cb)
        {
            conn.webSocket.OnConnectError += (a, b) => cb.Invoke(a, b);
            return this;
        }

        public DbConnectionBuilder<DbConnection, Reducer> OnDisconnect(Action<DbConnection, WebSocketCloseStatus?, WebSocketError?> cb)
        {
            conn.webSocket.OnClose += (code, error) => cb.Invoke(conn, code, error);
            return this;
        }
    }

    public interface IDbConnection
    {
        void Subscribe(ISubscriptionHandle handle, string query);
    }

    public abstract class DbConnectionBase<DbConnection, Reducer> : IDbConnection
        where DbConnection : DbConnectionBase<DbConnection, Reducer>, new()
    {
        public static DbConnectionBuilder<DbConnection, Reducer> Builder() => new();

        struct DbValue
        {
            public IDatabaseRow value;
            public byte[] bytes;

            public DbValue(IDatabaseRow value, byte[] bytes)
            {
                this.value = value;
                this.bytes = bytes;
            }
        }

        struct DbOp
        {
            public ClientCache.ITableCache table;
            public DbValue? delete;
            public DbValue? insert;
        }

        internal event Action<Identity, string>? onConnect;

        /// <summary>
        /// Called when an exception occurs when sending a message.
        /// </summary>
        public event Action<Exception>? onSendError;

        private readonly Dictionary<uint, ISubscriptionHandle> subscriptions = new();

        /// <summary>
        /// Invoked when a subscription is about to start being processed. This is called even before OnBeforeDelete.
        /// </summary>
        public event Action? onBeforeSubscriptionApplied;

        /// <summary>
        /// Invoked when a reducer is returned with an error and has no client-side handler.
        /// </summary>
        public event Action<ReducerEvent<Reducer>>? onUnhandledReducerError;

        /// <summary>
        /// Invoked when an event message is received or at the end of a transaction update.
        /// </summary>
        public event Action<ServerMessage>? onEvent;

        public readonly Address clientAddress = Address.Random();
        public Identity? clientIdentity { get; private set; }

        internal WebSocket webSocket;
        private bool connectionClosed;
        protected readonly ClientCache clientDB = new();

        protected abstract Reducer ToReducer(TransactionUpdate update);
        protected abstract IEventContext ToEventContext(Event<Reducer> reducerEvent);

        private readonly Dictionary<Guid, TaskCompletionSource<OneOffQueryResponse>> waitingOneOffQueries = new();

        private bool isClosing;
        private readonly Thread networkMessageProcessThread;
        public readonly Stats stats = new();

        protected DbConnectionBase()
        {
            var options = new ConnectOptions
            {
                //v1.bin.spacetimedb
                //v1.text.spacetimedb
                Protocol = "v1.bin.spacetimedb",
            };
            webSocket = new WebSocket(options);
            webSocket.OnMessage += OnMessageReceived;
            webSocket.OnSendError += a => onSendError?.Invoke(a);

            networkMessageProcessThread = new Thread(PreProcessMessages);
            networkMessageProcessThread.Start();
        }

        struct UnprocessedMessage
        {
            public byte[] bytes;
            public DateTime timestamp;
        }

        struct ProcessedMessage
        {
            public ServerMessage message;
            public List<DbOp> dbOps;
            public DateTime timestamp;
            public ReducerEvent<Reducer>? reducerEvent;
        }

        struct PreProcessedMessage
        {
            public ProcessedMessage processed;
            public Dictionary<Type, HashSet<byte[]>>? subscriptionInserts;
        }

        private readonly BlockingCollection<UnprocessedMessage> _messageQueue =
            new(new ConcurrentQueue<UnprocessedMessage>());

        private readonly BlockingCollection<PreProcessedMessage> _preProcessedNetworkMessages =
            new(new ConcurrentQueue<PreProcessedMessage>());

        internal bool HasPreProcessedMessage => _preProcessedNetworkMessages.Count > 0;

        private readonly CancellationTokenSource _preProcessCancellationTokenSource = new();
        private CancellationToken _preProcessCancellationToken => _preProcessCancellationTokenSource.Token;

        static DbValue Decode(ClientCache.ITableCache table, EncodedValue value) => value switch
        {
            EncodedValue.Binary(var bin) => new DbValue(table.DecodeValue(bin), bin),
            EncodedValue.Text(var text) => throw new InvalidOperationException("JavaScript messages aren't supported."),
            _ => throw new InvalidOperationException(),
        };

        private static readonly Status Committed = new Status.Committed(default);
        private static readonly Status OutOfEnergy = new Status.OutOfEnergy(default);

        void PreProcessMessages()
        {
            while (!isClosing)
            {
                try
                {
                    var message = _messageQueue.Take(_preProcessCancellationToken);
                    var preprocessedMessage = PreProcessMessage(message);
                    _preProcessedNetworkMessages.Add(preprocessedMessage, _preProcessCancellationToken);
                }
                catch (OperationCanceledException)
                {
                    return; // Normal shutdown
                }
            }

            PreProcessedMessage PreProcessMessage(UnprocessedMessage unprocessed)
            {
                var dbOps = new List<DbOp>();
                using var compressedStream = new MemoryStream(unprocessed.bytes);
                using var decompressedStream = new BrotliStream(compressedStream, CompressionMode.Decompress);
                using var binaryReader = new BinaryReader(decompressedStream);
                var message = new ServerMessage.BSATN().Read(binaryReader);

                ReducerEvent<Reducer>? reducerEvent = default;

                // This is all of the inserts
                Dictionary<System.Type, HashSet<byte[]>>? subscriptionInserts = null;
                // All row updates that have a primary key, this contains inserts, deletes and updates
                var primaryKeyChanges = new Dictionary<(System.Type tableType, object primaryKeyValue), DbOp>();

                HashSet<byte[]> GetInsertHashSet(System.Type tableType, int tableSize)
                {
                    if (!subscriptionInserts.TryGetValue(tableType, out var hashSet))
                    {
                        hashSet = new HashSet<byte[]>(capacity: tableSize, comparer: ByteArrayComparer.Instance);
                        subscriptionInserts[tableType] = hashSet;
                    }

                    return hashSet;
                }

                switch (message)
                {
                    case ServerMessage.InitialSubscription(var initialSubscription):
                        subscriptionInserts = new(capacity: initialSubscription.DatabaseUpdate.Tables.Sum(a => a.Inserts.Count));

                        // First apply all of the state
                        foreach (var update in initialSubscription.DatabaseUpdate.Tables)
                        {
                            var tableName = update.TableName;
                            var table = clientDB.GetTable(tableName);
                            if (table == null)
                            {
                                Log.Error($"Unknown table name: {tableName}");
                                continue;
                            }

                            if (update.Deletes.Count != 0)
                            {
                                Log.Warn("Non-insert during a subscription update!");
                            }

                            var hashSet = GetInsertHashSet(table.ClientTableType, initialSubscription.DatabaseUpdate.Tables.Count);

                            foreach (var row in update.Inserts)
                            {
                                switch (row)
                                {
                                    case EncodedValue.Binary(var bin):
                                        if (!hashSet.Add(bin))
                                        {
                                            // Ignore duplicate inserts in the same subscription update.
                                            continue;
                                        }

                                        var obj = table.DecodeValue(bin);
                                        var op = new DbOp
                                        {
                                            table = table,
                                            insert = new(obj, bin),
                                        };

                                        dbOps.Add(op);
                                        break;

                                    case EncodedValue.Text(var txt):
                                        Log.Warn("JavaScript messages are unsupported.");
                                        break;
                                }
                            }
                        }
                        break;

                    case ServerMessage.TransactionUpdate(var transactionUpdate):
                        // Convert the generic event arguments in to a domain specific event object
                        try
                        {
                            reducerEvent = new(
                                DateTimeOffset.FromUnixTimeMilliseconds((long)transactionUpdate.Timestamp.Microseconds / 1000),
                                transactionUpdate.Status switch
                                {
                                    UpdateStatus.Committed => Committed,
                                    UpdateStatus.OutOfEnergy => OutOfEnergy,
                                    UpdateStatus.Failed(var reason) => new Status.Failed(reason),
                                    _ => throw new InvalidOperationException()
                                },
                                transactionUpdate.CallerIdentity,
                                transactionUpdate.CallerAddress,
                                transactionUpdate.EnergyQuantaUsed.Quanta,
                                ToReducer(transactionUpdate));
                        }
                        catch (Exception e)
                        {
                            Log.Exception(e);
                        }

                        if (transactionUpdate.Status is UpdateStatus.Committed(var committed))
                        {
                            primaryKeyChanges = new();

                            // First apply all of the state
                            foreach (var update in committed.Tables)
                            {
                                var tableName = update.TableName;
                                var table = clientDB.GetTable(tableName);
                                if (table == null)
                                {
                                    Log.Error($"Unknown table name: {tableName}");
                                    continue;
                                }

                                foreach (var row in update.Inserts)
                                {
                                    var op = new DbOp { table = table, insert = Decode(table, row) };
                                    var pk = table.Handle.GetPrimaryKey(op.insert.Value.value);
                                    if (pk != null)
                                    {
                                        // Compound key that we use for lookup.
                                        // Consists of type of the table (for faster comparison that string names) + actual primary key of the row.
                                        var key = (table.ClientTableType, pk);

                                        if (primaryKeyChanges.TryGetValue(key, out var oldOp))
                                        {
                                            if ((op.insert is not null && oldOp.insert is not null) || (op.delete is not null && oldOp.delete is not null))
                                            {
                                                Log.Warn($"Update with the same primary key was applied multiple times! tableName={tableName}");
                                                // TODO(jdetter): Is this a correctable error? This would be a major error on the
                                                // SpacetimeDB side.
                                                continue;
                                            }

                                            var (insertOp, deleteOp) = op.insert is not null ? (op, oldOp) : (oldOp, op);
                                            op = new DbOp
                                            {
                                                table = insertOp.table,
                                                delete = deleteOp.delete,
                                                insert = insertOp.insert,
                                            };
                                        }
                                        primaryKeyChanges[key] = op;
                                    }
                                    else
                                    {
                                        dbOps.Add(op);
                                    }
                                }

                                foreach (var row in update.Deletes)
                                {
                                    var op = new DbOp { table = table, delete = Decode(table, row) };
                                    var pk = table.Handle.GetPrimaryKey(op.delete.Value.value);
                                    if (pk != null)
                                    {
                                        // Compound key that we use for lookup.
                                        // Consists of type of the table (for faster comparison that string names) + actual primary key of the row.
                                        var key = (table.ClientTableType, pk);

                                        if (primaryKeyChanges.TryGetValue(key, out var oldOp))
                                        {
                                            if ((op.insert is not null && oldOp.insert is not null) || (op.delete is not null && oldOp.delete is not null))
                                            {
                                                Log.Warn($"Update with the same primary key was applied multiple times! tableName={tableName}");
                                                // TODO(jdetter): Is this a correctable error? This would be a major error on the
                                                // SpacetimeDB side.
                                                continue;
                                            }

                                            var (insertOp, deleteOp) = op.insert is not null ? (op, oldOp) : (oldOp, op);
                                            op = new DbOp
                                            {
                                                table = insertOp.table,
                                                delete = deleteOp.delete,
                                                insert = insertOp.insert,
                                            };
                                        }
                                        primaryKeyChanges[key] = op;
                                    }
                                    else
                                    {
                                        dbOps.Add(op);
                                    }
                                }
                            }

                            // Combine primary key updates and non-primary key updates
                            dbOps.AddRange(primaryKeyChanges.Values);
                        }
                        break;
                    case ServerMessage.IdentityToken(var identityToken):
                        break;
                    case ServerMessage.OneOffQueryResponse(var resp):
                        /// This case does NOT produce a list of DBOps, because it should not modify the client cache state!
                        var messageId = new Guid(resp.MessageId);

                        if (!waitingOneOffQueries.Remove(messageId, out var resultSource))
                        {
                            Log.Error($"Response to unknown one-off-query: {messageId}");
                            break;
                        }

                        resultSource.SetResult(resp);
                        break;
                    default:
                        throw new InvalidOperationException();
                }

                // Logger.LogWarning($"Total Updates preprocessed: {totalUpdateCount}");
                return new PreProcessedMessage
                {
                    processed = new ProcessedMessage { message = message, dbOps = dbOps, timestamp = unprocessed.timestamp, reducerEvent = reducerEvent },
                    subscriptionInserts = subscriptionInserts,
                };
            }
        }

        ProcessedMessage CalculateStateDiff(PreProcessedMessage preProcessedMessage)
        {
            var processed = preProcessedMessage.processed;

            // Perform the state diff, this has to be done on the main thread because we have to touch
            // the client cache.
            if (preProcessedMessage.subscriptionInserts is { } subscriptionInserts)
            {
                foreach (var table in clientDB.GetTables())
                {
                    if (!subscriptionInserts.TryGetValue(table.ClientTableType, out var hashSet))
                    {
                        continue;
                    }

                    foreach (var (rowBytes, oldValue) in table.Where(kv => !hashSet.Contains(kv.Key)))
                    {
                        processed.dbOps.Add(new DbOp
                        {
                            table = table,
                            // This is a row that we had before, but we do not have it now.
                            // This must have been a delete.
                            delete = new(oldValue, rowBytes),
                        });
                    }
                }
            }

            return processed;
        }

        public void Close()
        {
            isClosing = true;
            connectionClosed = true;
            webSocket.Close();
            _preProcessCancellationTokenSource.Cancel();
        }

        /// <summary>
        /// Connect to a remote spacetime instance.
        /// </summary>
        /// <param name="uri"> URI of the SpacetimeDB server (ex: https://testnet.spacetimedb.com)
        /// <param name="addressOrName">The name or address of the database to connect to</param>
        public void Connect(string? token, string uri, string addressOrName)
        {
            isClosing = false;

            uri = uri.Replace("http://", "ws://");
            uri = uri.Replace("https://", "wss://");
            if (!uri.StartsWith("ws://") && !uri.StartsWith("wss://"))
            {
                uri = $"ws://{uri}";
            }

            Log.Info($"SpacetimeDBClient: Connecting to {uri} {addressOrName}");
            Task.Run(async () =>
            {
                try
                {
                    await webSocket.Connect(token, uri, addressOrName, clientAddress);
                }
                catch (Exception e)
                {
                    if (connectionClosed)
                    {
                        Log.Info("Connection closed gracefully.");
                        return;
                    }

                    Log.Exception(e);
                }
            });
        }

        private void OnMessageProcessCompleteUpdate(IEventContext eventContext, List<DbOp> dbOps)
        {
            // First trigger OnBeforeDelete
            foreach (var update in dbOps)
            {
                if (update is { delete: { value: var oldValue }, insert: null })
                {
                    try
                    {
                        update.table.Handle.InvokeBeforeDelete(eventContext, oldValue);
                    }
                    catch (Exception e)
                    {
                        Log.Exception(e);
                    }
                }
            }

            // Apply all of the state
            for (var i = 0; i < dbOps.Count; i++)
            {
                // TODO: Reimplement updates when we add support for primary keys
                var update = dbOps[i];

                if (update.delete is { } delete)
                {
                    if (update.table.DeleteEntry(delete.bytes))
                    {
                        update.table.Handle.InternalInvokeValueDeleted(delete.value);
                    }
                    else
                    {
                        update.delete = null;
                        dbOps[i] = update;
                    }
                }

                if (update.insert is { } insert)
                {
                    if (update.table.InsertEntry(insert.bytes, insert.value))
                    {
                        update.table.Handle.InternalInvokeValueInserted(insert.value);
                    }
                    else
                    {
                        update.insert = null;
                        dbOps[i] = update;
                    }
                }
            }

            // Send out events
            foreach (var dbOp in dbOps)
            {
                try
                {
                    switch (dbOp)
                    {
                        case { insert: { value: var newValue }, delete: { value: var oldValue } }:
                            {
                                dbOp.table.Handle.InvokeUpdate(eventContext, oldValue, newValue);
                                break;
                            }

                        case { insert: { value: var newValue } }:
                            dbOp.table.Handle.InvokeInsert(eventContext, newValue);
                            break;

                        case { delete: { value: var oldValue } }:
                            dbOp.table.Handle.InvokeDelete(eventContext, oldValue);
                            break;
                    }
                }
                catch (Exception e)
                {
                    Log.Exception(e);
                }
            }
        }

        protected abstract bool Dispatch(IEventContext context, Reducer reducer);

        private void OnMessageProcessComplete(PreProcessedMessage preProcessed)
        {
            var processed = CalculateStateDiff(preProcessed);
            var message = processed.message;
            var dbOps = processed.dbOps;
            var timestamp = processed.timestamp;

            switch (message)
            {
                case ServerMessage.InitialSubscription(var initialSubscription):
                    {
                        onBeforeSubscriptionApplied?.Invoke();
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.InitialSubscription)}");
                        stats.SubscriptionRequestTracker.FinishTrackingRequest(initialSubscription.RequestId);
                        var eventContext = ToEventContext(new Event<Reducer>.SubscribeApplied());
                        OnMessageProcessCompleteUpdate(eventContext, dbOps);
                        if (subscriptions.TryGetValue(initialSubscription.RequestId, out var subscription))
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
                        break;
                    }
                case ServerMessage.TransactionUpdate(var transactionUpdate):
                    {
                        var reducer = transactionUpdate.ReducerCall.ReducerName;
                        stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.TransactionUpdate)},reducer={reducer}");
                        var hostDuration = TimeSpan.FromMilliseconds(transactionUpdate.HostExecutionDurationMicros / 1000.0d);
                        stats.AllReducersTracker.InsertRequest(hostDuration, $"reducer={reducer}");
                        var callerIdentity = transactionUpdate.CallerIdentity;
                        if (callerIdentity == clientIdentity)
                        {
                            // This was a request that we initiated
                            var requestId = transactionUpdate.ReducerCall.RequestId;
                            if (!stats.ReducerRequestTracker.FinishTrackingRequest(requestId))
                            {
                                Log.Warn($"Failed to finish tracking reducer request: {requestId}");
                            }
                        }

                        if (processed.reducerEvent is not { } reducerEvent)
                        {
                            // If we are here, an error about unknown reducer should have already been logged, so nothing to do.
                            break;
                        }

                        var eventContext = ToEventContext(new Event<Reducer>.Reducer(reducerEvent));
                        OnMessageProcessCompleteUpdate(eventContext, dbOps);
                        try
                        {
                            onEvent?.Invoke(message);
                        }
                        catch (Exception e)
                        {
                            Log.Exception(e);
                        }

                        var reducerFound = false;
                        try
                        {
                            reducerFound = Dispatch(eventContext, reducerEvent.Reducer);
                        }
                        catch (Exception e)
                        {
                            Log.Exception(e);
                        }

                        if (!reducerFound && transactionUpdate.Status is UpdateStatus.Failed(var failed))
                        {
                            try
                            {
                                onUnhandledReducerError?.Invoke(reducerEvent);
                            }
                            catch (Exception e)
                            {
                                Log.Exception(e);
                            }
                        }
                        break;
                    }
                case ServerMessage.IdentityToken(var identityToken):
                    try
                    {
                        clientIdentity = identityToken.Identity;
                        onConnect?.Invoke(identityToken.Identity, identityToken.Token);
                    }
                    catch (Exception e)
                    {
                        Log.Exception(e);
                    }
                    break;

                case ServerMessage.OneOffQueryResponse:
                    try
                    {
                        onEvent?.Invoke(message);
                    }
                    catch (Exception e)
                    {
                        Log.Exception(e);
                    }
                    break;

                default:
                    throw new InvalidOperationException();
            }
        }

        // Note: this method is called from unit tests.
        internal void OnMessageReceived(byte[] bytes, DateTime timestamp) =>
            _messageQueue.Add(new UnprocessedMessage { bytes = bytes, timestamp = timestamp });

        public void InternalCallReducer<T>(T args)
            where T : IReducerArgs, new()
        {
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot call reducer, not connected to server!");
                return;
            }

            webSocket.Send(new ClientMessage.CallReducer(
                new CallReducer
                {
                    RequestId = stats.ReducerRequestTracker.StartTrackingRequest(args.ReducerName),
                    Reducer = args.ReducerName,
                    Args = new EncodedValue.Binary(IStructuralReadWrite.ToBytes(args))
                }
            ));
        }

        void IDbConnection.Subscribe(ISubscriptionHandle handle, string query)
        {
            if (!webSocket.IsConnected)
            {
                Log.Error("Cannot subscribe, not connected to server!");
                return;
            }

            var id = stats.SubscriptionRequestTracker.StartTrackingRequest();
            subscriptions[id] = handle;
            webSocket.Send(new ClientMessage.Subscribe(
                new Subscribe
                {
                    RequestId = id,
                    QueryStrings = { query }
                }
            ));
        }

        /// Usage: SpacetimeDBClientBase.instance.OneOffQuery<Message>("WHERE sender = \"bob\"");
        public async Task<T[]> OneOffQuery<T>(string query)
            where T : IDatabaseRow, new()
        {
            var messageId = Guid.NewGuid();
            var type = typeof(T);
            var resultSource = new TaskCompletionSource<OneOffQueryResponse>();
            waitingOneOffQueries[messageId] = resultSource;

            // unsanitized here, but writes will be prevented serverside.
            // the best they can do is send multiple selects, which will just result in them getting no data back.
            string queryString = $"SELECT * FROM {type.Name} {query}";

            var requestId = stats.OneOffRequestTracker.StartTrackingRequest();
            webSocket.Send(new ClientMessage.OneOffQuery(new OneOffQuery
            {
                MessageId = messageId.ToByteArray(),
                QueryString = queryString,
            }));

            // Suspend for an arbitrary amount of time
            var result = await resultSource.Task;

            if (!stats.OneOffRequestTracker.FinishTrackingRequest(requestId))
            {
                Log.Warn($"Failed to finish tracking one off request: {requestId}");
            }

            T[] LogAndThrow(string error)
            {
                error = $"While processing one-off-query `{queryString}`, ID {messageId}: {error}";
                Log.Error(error);
                throw new Exception(error);
            }

            // The server got back to us
            if (result.Error != null && result.Error != "")
            {
                return LogAndThrow($"Server error: {result.Error}");
            }

            if (result.Tables.Count != 1)
            {
                return LogAndThrow($"Expected a single table, but got {result.Tables.Count}");
            }

            var resultTable = result.Tables[0];
            var cacheTable = clientDB.GetTable(resultTable.TableName);

            if (cacheTable?.ClientTableType != type)
            {
                return LogAndThrow($"Mismatched result type, expected {type} but got {resultTable.TableName}");
            }

            return resultTable.Rows.Select(BSATNHelpers.Decode<T>).ToArray();
        }

        public bool IsConnected() => webSocket.IsConnected;

        public void Update()
        {
            webSocket.Update();
            while (_preProcessedNetworkMessages.TryTake(out var preProcessedMessage))
            {
                OnMessageProcessComplete(preProcessedMessage);
            }
        }
    }
}
