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
using SpacetimeDB.ClientApi;
using Thread = System.Threading.Thread;

namespace SpacetimeDB
{
    public abstract class SpacetimeDBClientBase<ReducerEvent>
        where ReducerEvent : ReducerEventBase
    {
        struct DbValue
        {
            public IDatabaseTable value;
            public byte[] bytes;

            public DbValue(IDatabaseTable value, byte[] bytes)
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

        /// <summary>
        /// Called when a connection is established to a spacetimedb instance.
        /// </summary>
        public event Action? onConnect;

        /// <summary>
        /// Called when a connection attempt fails.
        /// </summary>
        public event Action<WebSocketError?, string>? onConnectError;

        /// <summary>
        /// Called when an exception occurs when sending a message.
        /// </summary>
        public event Action<Exception>? onSendError;

        /// <summary>
        /// Called when a connection that was established has disconnected.
        /// </summary>
        public event Action<WebSocketCloseStatus?, WebSocketError?>? onDisconnect;

        /// <summary>
        /// Invoked when a subscription is about to start being processed. This is called even before OnBeforeDelete.
        /// </summary>
        public event Action? onBeforeSubscriptionApplied;

        /// <summary>
        /// Invoked when the local client cache is updated as a result of changes made to the subscription queries.
        /// </summary>
        public event Action? onSubscriptionApplied;

        /// <summary>
        /// Invoked when a reducer is returned with an error and has no client-side handler.
        /// </summary>
        public event Action<ReducerEvent>? onUnhandledReducerError;

        /// <summary>
        /// Called when we receive an identity from the server
        /// </summary>
        public event Action<string, Identity, Address>? onIdentityReceived;

        /// <summary>
        /// Invoked when an event message is received or at the end of a transaction update.
        /// </summary>
        public event Action<ServerMessage>? onEvent;

        public readonly Address clientAddress = Address.Random();
        public Identity clientIdentity { get; private set; }

        private SpacetimeDB.WebSocket webSocket;
        private bool connectionClosed;
        protected readonly ClientCache clientDB = new();

        protected abstract ReducerEvent ReducerEventFromDbEvent(TransactionUpdate dbEvent);

        private readonly Dictionary<Guid, TaskCompletionSource<OneOffQueryResponse>> waitingOneOffQueries = new();

        private bool isClosing;
        private readonly Thread networkMessageProcessThread;
        public readonly Stats stats = new();

        protected SpacetimeDBClientBase()
        {
            var options = new ConnectOptions
            {
                //v1.bin.spacetimedb
                //v1.text.spacetimedb
                Protocol = "v1.bin.spacetimedb",
            };
            webSocket = new WebSocket(options);
            webSocket.OnMessage += OnMessageReceived;
            webSocket.OnClose += (code, error) => onDisconnect?.Invoke(code, error);
            webSocket.OnConnect += () => onConnect?.Invoke();
            webSocket.OnConnectError += (a, b) => onConnectError?.Invoke(a, b);
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
            public ReducerEvent? reducerEvent;
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

                ReducerEvent? reducerEvent = null;

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
                                Logger.LogError($"Unknown table name: {tableName}");
                                continue;
                            }

                            if (update.Deletes.Count != 0)
                            {
                                Logger.LogWarning("Non-insert during a subscription update!");
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
                                        Logger.LogWarning("JavaScript messages are unsupported.");
                                        break;
                                }
                            }
                        }

                        break;

                    case ServerMessage.TransactionUpdate(var transactionUpdate):
                        switch (transactionUpdate.Status)
                        {
                            case UpdateStatus.Committed(var committed):
                                primaryKeyChanges = new();

                                // First apply all of the state
                                foreach (var update in committed.Tables)
                                {
                                    var tableName = update.TableName;
                                    var table = clientDB.GetTable(tableName);
                                    if (table == null)
                                    {
                                        Logger.LogError($"Unknown table name: {tableName}");
                                        continue;
                                    }

                                    foreach (var row in update.Inserts)
                                    {
                                        var op = new DbOp { table = table, insert = Decode(table, row) };

                                        if (op.insert.Value.value is IDatabaseTableWithPrimaryKey objWithPk)
                                        {
                                            // Compound key that we use for lookup.
                                            // Consists of type of the table (for faster comparison that string names) + actual primary key of the row.
                                            var key = (table.ClientTableType, objWithPk.GetPrimaryKeyValue());

                                            if (primaryKeyChanges.TryGetValue(key, out var oldOp))
                                            {
                                                if ((op.insert is not null && oldOp.insert is not null) || (op.delete is not null && oldOp.delete is not null))
                                                {
                                                    Logger.LogWarning($"Update with the same primary key was applied multiple times! tableName={tableName}");
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

                                        if (op.delete.Value.value is IDatabaseTableWithPrimaryKey objWithPk)
                                        {
                                            // Compound key that we use for lookup.
                                            // Consists of type of the table (for faster comparison that string names) + actual primary key of the row.
                                            var key = (table.ClientTableType, objWithPk.GetPrimaryKeyValue());

                                            if (primaryKeyChanges.TryGetValue(key, out var oldOp))
                                            {
                                                if ((op.insert is not null && oldOp.insert is not null) || (op.delete is not null && oldOp.delete is not null))
                                                {
                                                    Logger.LogWarning($"Update with the same primary key was applied multiple times! tableName={tableName}");
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

                                // Convert the generic event arguments in to a domain specific event object
                                try
                                {
                                    reducerEvent = ReducerEventFromDbEvent(transactionUpdate);
                                }
                                catch (Exception e)
                                {
                                    Logger.LogException(e);
                                }
                                break;
                            case UpdateStatus.Failed(var failed):
                                break;
                            case UpdateStatus.OutOfEnergy(var outOfEnergy):
                                Logger.LogWarning("Failed to execute reducer: out of energy.");
                                break;
                            default:
                                throw new InvalidOperationException();
                        }
                        break;
                    case ServerMessage.IdentityToken(var identityToken):
                        break;
                    case ServerMessage.OneOffQueryResponse(var resp):
                        /// This case does NOT produce a list of DBOps, because it should not modify the client cache state!
                        var messageId = new Guid(resp.MessageId);

                        if (!waitingOneOffQueries.Remove(messageId, out var resultSource))
                        {
                            Logger.LogError($"Response to unknown one-off-query: {messageId}");
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

            Logger.Log($"SpacetimeDBClient: Connecting to {uri} {addressOrName}");
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
                        Logger.Log("Connection closed gracefully.");
                        return;
                    }

                    Logger.LogException(e);
                }
            });
        }


        private void OnMessageProcessCompleteUpdate(ReducerEvent? dbEvent, List<DbOp> dbOps)
        {
            // First trigger OnBeforeDelete
            foreach (var update in dbOps)
            {
                if (update is { delete: { value: var oldValue }, insert: null })
                {
                    try
                    {
                        oldValue.OnBeforeDeleteEvent(dbEvent!);
                    }
                    catch (Exception e)
                    {
                        Logger.LogException(e);
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
                        delete.value.InternalOnValueDeleted();
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
                        insert.value.InternalOnValueInserted();
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
                                // If we matched an update, these values must have primary keys.
                                var newValue_ = (IDatabaseTableWithPrimaryKey)newValue;
                                var oldValue_ = (IDatabaseTableWithPrimaryKey)oldValue;
                                oldValue_.OnUpdateEvent(newValue_, dbEvent);
                                break;
                            }

                        case { insert: { value: var newValue } }:
                            newValue.OnInsertEvent(dbEvent);
                            break;

                        case { delete: { value: var oldValue } }:
                            oldValue.OnDeleteEvent(dbEvent);
                            break;
                    }
                }
                catch (Exception e)
                {
                    Logger.LogException(e);
                }
            }
        }

        private void OnMessageProcessComplete(PreProcessedMessage preProcessed)
        {
            var processed = CalculateStateDiff(preProcessed);
            var message = processed.message;
            var dbOps = processed.dbOps;
            var timestamp = processed.timestamp;

            switch (message)
            {
                case ServerMessage.InitialSubscription(var initialSubscription):
                    onBeforeSubscriptionApplied?.Invoke();
                    stats.ParseMessageTracker.InsertRequest(timestamp, $"type={nameof(ServerMessage.InitialSubscription)}");
                    stats.SubscriptionRequestTracker.FinishTrackingRequest(initialSubscription.RequestId);
                    OnMessageProcessCompleteUpdate(null, dbOps);
                    try
                    {
                        onSubscriptionApplied?.Invoke();
                    }
                    catch (Exception e)
                    {
                        Logger.LogException(e);
                    }
                    break;
                case ServerMessage.TransactionUpdate(var transactionUpdate):
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
                            Logger.LogWarning($"Failed to finish tracking reducer request: {requestId}");
                        }
                    }
                    OnMessageProcessCompleteUpdate(processed.reducerEvent, dbOps);
                    try
                    {
                        onEvent?.Invoke(message);
                    }
                    catch (Exception e)
                    {
                        Logger.LogException(e);
                    }

                    if (processed.reducerEvent is not { } reducerEvent)
                    {
                        // If we are here, an error about unknown reducer should have already been logged, so nothing to do.
                        break;
                    }

                    var reducerFound = false;
                    try
                    {
                        reducerFound = reducerEvent.InvokeHandler();
                    }
                    catch (Exception e)
                    {
                        Logger.LogException(e);
                    }

                    if (!reducerFound && transactionUpdate.Status is UpdateStatus.Failed(var failed))
                    {
                        try
                        {
                            onUnhandledReducerError?.Invoke(reducerEvent);
                        }
                        catch (Exception e)
                        {
                            Logger.LogException(e);
                        }
                    }
                    break;
                case ServerMessage.IdentityToken(var identityToken):
                    try
                    {
                        clientIdentity = identityToken.Identity;
                        var address = identityToken.Address;
                        onIdentityReceived?.Invoke(identityToken.Token, clientIdentity, address);
                    }
                    catch (Exception e)
                    {
                        Logger.LogException(e);
                    }
                    break;
                case ServerMessage.OneOffQueryResponse(var _):
                    try
                    {
                        onEvent?.Invoke(message);
                    }
                    catch (Exception e)
                    {
                        Logger.LogException(e);
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
            where T : IReducerArgsBase, new()
        {
            if (!webSocket.IsConnected)
            {
                Logger.LogError("Cannot call reducer, not connected to server!");
                return;
            }

            var o = new MemoryStream();
            var bw = new BinaryWriter(o);
            args.WriteFields(bw);

            webSocket.Send(new ClientMessage.CallReducer(
                new CallReducer
                {
                    RequestId = stats.ReducerRequestTracker.StartTrackingRequest(args.ReducerName),
                    Reducer = args.ReducerName,
                    Args = new EncodedValue.Binary(o.ToArray())
                }
            ));
        }

        public void Subscribe(List<string> queries)
        {
            if (!webSocket.IsConnected)
            {
                Logger.LogError("Cannot subscribe, not connected to server!");
                return;
            }

            var request = new Subscribe
            {
                RequestId = stats.SubscriptionRequestTracker.StartTrackingRequest(),
            };
            request.QueryStrings.AddRange(queries);
            webSocket.Send(new ClientMessage.Subscribe(request));
        }

        /// Usage: SpacetimeDBClientBase.instance.OneOffQuery<Message>("WHERE sender = \"bob\"");
        public async Task<T[]> OneOffQuery<T>(string query)
            where T : IDatabaseTable, IStructuralReadWrite, new()
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
                Logger.LogWarning($"Failed to finish tracking one off request: {requestId}");
            }

            T[] LogAndThrow(string error)
            {
                error = $"While processing one-off-query `{queryString}`, ID {messageId}: {error}";
                Logger.LogError(error);
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
