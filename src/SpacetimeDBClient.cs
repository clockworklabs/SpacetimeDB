using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Net.WebSockets;
using System.Reflection;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using ClientApi;
using Newtonsoft.Json;
using SpacetimeDB.SATS;
using Thread = System.Threading.Thread;

namespace SpacetimeDB
{
    public class SpacetimeDBClient
    {
        public class ReducerCallRequest
        {
            public string fn;
            public object[] args;
        }

        public class SubscriptionRequest
        {
            public string subscriptionQuery;
        }

        struct DbValue
        {
            public object value;
            public byte[] bytes;

            public DbValue(object value, byte[] bytes)
            {
                this.value = value;
                this.bytes = bytes;
            }
        }

        struct DbOp
        {
            public ClientCache.TableCache table;
            public DbValue? delete;
            public DbValue? insert;
        }

        /// <summary>
        /// Called when a connection is established to a spacetimedb instance.
        /// </summary>
        public event Action onConnect;

        /// <summary>
        /// Called when a connection attempt fails.
        /// </summary>
        public event Action<WebSocketError?, string> onConnectError;

        /// <summary>
        /// Called when an exception occurs when sending a message.
        /// </summary>
        public event Action<Exception> onSendError;

        /// <summary>
        /// Called when a connection that was established has disconnected.
        /// </summary>
        public event Action<WebSocketCloseStatus?, WebSocketError?> onDisconnect;

        /// <summary>
        /// Invoked when a subscription is about to start being processed. This is called even before OnBeforeDelete.
        /// </summary>
        public event Action onBeforeSubscriptionApplied;

        /// <summary>
        /// Invoked when the local client cache is updated as a result of changes made to the subscription queries.
        /// </summary>
        public event Action onSubscriptionApplied;

        /// <summary>
        /// Invoked when a reducer is returned with an error and has no client-side handler.
        /// </summary>
        public event Action<ReducerEventBase> onUnhandledReducerError;

        /// <summary>
        /// Called when we receive an identity from the server
        /// </summary>
        public event Action<string, Identity, Address> onIdentityReceived;

        /// <summary>
        /// Invoked when an event message is received or at the end of a transaction update.
        /// </summary>
        public event Action<ClientApi.Event> onEvent;

        public readonly Address clientAddress = Address.Random();

        private SpacetimeDB.WebSocket webSocket;
        private bool connectionClosed;
        public static ClientCache clientDB;

        private static Dictionary<string, Func<ClientApi.Event, bool>> reducerEventCache = new();

        private static Dictionary<string, Action<ClientApi.Event>> deserializeEventCache = new();

        private static Dictionary<Guid, TaskCompletionSource<OneOffQueryResponse>> waitingOneOffQueries = new();

        private bool isClosing;
        private Thread networkMessageProcessThread;
        private Thread stateDiffProcessThread;

        public static readonly SpacetimeDBClient instance = new();

        public Type FindReducerType()
        {
            // Get all loaded assemblies
            Assembly[] assemblies = AppDomain.CurrentDomain.GetAssemblies();

            // Iterate over each assembly and search for the type
            foreach (Assembly assembly in assemblies)
            {
                // Get all types in the assembly
                Type[] types = assembly.GetTypes();

                // Search for the class with the attribute ReducerClass
                foreach (Type type in types)
                {
                    if (type.GetCustomAttribute<ReducerClassAttribute>() != null)
                    {
                        return type;
                    }
                }
            }

            // If the type is not found in any assembly, return null or throw an exception
            return null;
        }

        protected SpacetimeDBClient()
        {
            var options = new SpacetimeDB.ConnectOptions
            {
                //v1.bin.spacetimedb
                //v1.text.spacetimedb
                Protocol = "v1.bin.spacetimedb",
            };
            webSocket = new SpacetimeDB.WebSocket(options);
            webSocket.OnMessage += OnMessageReceived;
            webSocket.OnClose += (code, error) => onDisconnect?.Invoke(code, error);
            webSocket.OnConnect += () => onConnect?.Invoke();
            webSocket.OnConnectError += (a, b) => onConnectError?.Invoke(a, b);
            webSocket.OnSendError += a => onSendError?.Invoke(a);

            clientDB = new ClientCache();

            var type = typeof(IDatabaseTable);
            var types = AppDomain.CurrentDomain.GetAssemblies().SelectMany(s => s.GetTypes())
                .Where(p => type.IsAssignableFrom(p));
            foreach (var @class in types)
            {
                if (!@class.IsClass)
                {
                    continue;
                }

                var algebraicTypeFunc = @class.GetMethod("GetAlgebraicType", BindingFlags.Static | BindingFlags.Public);
                var algebraicValue = algebraicTypeFunc!.Invoke(null, null) as AlgebraicType;
                var conversionFunc = @class.GetMethods()
                    .FirstOrDefault(a => a.Name == "op_Explicit" &&
                                         a.GetParameters().Length > 0 &&
                                         a.GetParameters()[0].ParameterType ==
                                         typeof(AlgebraicValue));
                clientDB.AddTable(@class, algebraicValue,
                    a => { return conversionFunc!.Invoke(null, new object[] { a }); });
            }

            var reducerType = FindReducerType();
            if (reducerType != null)
            {
                // cache all our reducer events by their function name
                foreach (var methodInfo in reducerType.GetMethods())
                {
                    if (methodInfo.GetCustomAttribute<ReducerCallbackAttribute>() is
                        { } reducerEvent)
                    {
                        reducerEventCache.Add(reducerEvent.FunctionName,
                            (Func<ClientApi.Event, bool>)methodInfo.CreateDelegate(
                                typeof(Func<ClientApi.Event, bool>)));
                    }

                    if (methodInfo.GetCustomAttribute<DeserializeEventAttribute>() is
                        { } deserializeEvent)
                    {
                        deserializeEventCache.Add(deserializeEvent.FunctionName,
                            (Action<ClientApi.Event>)methodInfo.CreateDelegate(typeof(Action<ClientApi.Event>)));
                    }
                }
            }
            else
            {
                Logger.LogError($"Could not find reducer type. Have you run spacetime generate?");
            }

            _preProcessCancellationToken = _preProcessCancellationTokenSource.Token;
            networkMessageProcessThread = new Thread(PreProcessMessages);
            networkMessageProcessThread.Start();

            _stateDiffCancellationToken = _stateDiffCancellationTokenSource.Token;
            stateDiffProcessThread = new Thread(ExecuteStateDiff);
            stateDiffProcessThread.Start();
        }

        struct PreProcessedMessage
        {
            public Message message;
            public List<DbOp> dbOps;
            public Dictionary<System.Type, HashSet<byte[]>> inserts;
        }

        private readonly BlockingCollection<byte[]> _messageQueue =
            new BlockingCollection<byte[]>(new ConcurrentQueue<byte[]>());

        private readonly BlockingCollection<PreProcessedMessage> _preProcessedNetworkMessages =
            new BlockingCollection<PreProcessedMessage>(new ConcurrentQueue<PreProcessedMessage>());

        private CancellationTokenSource _preProcessCancellationTokenSource = new CancellationTokenSource();
        private CancellationToken _preProcessCancellationToken;

        void PreProcessMessages()
        {
            while (!isClosing)
            {
                try
                {
                    var bytes = _messageQueue.Take(_preProcessCancellationToken);
                    var preprocessedMessage = PreProcessMessage(bytes);
                    _preProcessedNetworkMessages.Add(preprocessedMessage, _preProcessCancellationToken);
                }
                catch (OperationCanceledException)
                {
                    // Normal shutdown
                    return;
                }
            }

            PreProcessedMessage PreProcessMessage(byte[] bytes)
            {
                var dbOps = new List<DbOp>();
                using var compressedStream = new MemoryStream(bytes);
                using var decompressedStream = new BrotliStream(compressedStream, CompressionMode.Decompress);
                var message = Message.Parser.ParseFrom(decompressedStream);
                using var stream = new MemoryStream();
                using var reader = new BinaryReader(stream);

                // This is all of the inserts
                Dictionary<System.Type, HashSet<byte[]>> subscriptionInserts = null;
                // All row updates that have a primary key, this contains inserts, deletes and updates
                var primaryKeyChanges = new Dictionary<(System.Type tableType, object primaryKeyValue), DbOp>();

                HashSet<byte[]> GetInsertHashSet(System.Type tableType, int tableSize)
                {
                    if (!subscriptionInserts.TryGetValue(tableType, out var hashSet))
                    {
                        hashSet = new HashSet<byte[]>(capacity: tableSize, comparer: new ByteArrayComparer());
                        subscriptionInserts[tableType] = hashSet;
                    }

                    return hashSet;
                }

                SubscriptionUpdate subscriptionUpdate = null;
                switch (message.TypeCase)
                {
                    case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                        subscriptionUpdate = message.SubscriptionUpdate;
                        subscriptionInserts = new(capacity: subscriptionUpdate.TableUpdates.Sum(a => a.TableRowOperations.Count));
                        // First apply all of the state
                        foreach (var update in subscriptionUpdate.TableUpdates)
                        {
                            var tableName = update.TableName;
                            var table = clientDB.GetTable(tableName);
                            if (table == null)
                            {
                                Logger.LogError($"Unknown table name: {tableName}");
                                continue;
                            }

                            var hashSet = GetInsertHashSet(table.ClientTableType, subscriptionUpdate.TableUpdates.Count);

                            foreach (var row in update.TableRowOperations)
                            {
                                var rowBytes = row.Row.ToByteArray();

                                if (row.Op != TableRowOperation.Types.OperationType.Insert)
                                {
                                    Logger.LogWarning("Non-insert during a subscription update!");
                                    continue;
                                }

                                if (!hashSet.Add(rowBytes))
                                {
                                    // Ignore duplicate inserts in the same subscription update.
                                    continue;
                                }

                                stream.Position = 0;
                                stream.Write(rowBytes, 0, rowBytes.Length);
                                stream.Position = 0;
                                stream.SetLength(rowBytes.Length);
                                var deserializedRow = AlgebraicValue.Deserialize(table.RowSchema, reader);
                                if (deserializedRow == null)
                                {
                                    throw new Exception("Failed to deserialize row");
                                }

                                table.SetAndForgetDecodedValue(deserializedRow, out var obj);
                                var op = new DbOp
                                {
                                    table = table,
                                    insert = new(obj, rowBytes),
                                };

                                dbOps.Add(op);
                            }
                        }

                        break;

                    case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                        subscriptionUpdate = message.TransactionUpdate.SubscriptionUpdate;
                        // First apply all of the state
                        foreach (var update in subscriptionUpdate.TableUpdates)
                        {
                            var tableName = update.TableName;
                            var table = clientDB.GetTable(tableName);
                            if (table == null)
                            {
                                Logger.LogError($"Unknown table name: {tableName}");
                                continue;
                            }

                            foreach (var row in update.TableRowOperations)
                            {
                                var rowBytes = row.Row.ToByteArray();
                                stream.Position = 0;
                                stream.Write(rowBytes, 0, rowBytes.Length);
                                stream.Position = 0;
                                stream.SetLength(rowBytes.Length);
                                var deserializedRow = AlgebraicValue.Deserialize(table.RowSchema, reader);
                                if (deserializedRow == null)
                                {
                                    throw new Exception("Failed to deserialize row");
                                }

                                table.SetAndForgetDecodedValue(deserializedRow, out var obj);
                                var primaryKeyValue = table.GetPrimaryKeyValue(obj);

                                var op = new DbOp { table = table };

                                var dbValue = new DbValue(obj, rowBytes);

                                if (row.Op == TableRowOperation.Types.OperationType.Insert)
                                {
                                    op.insert = dbValue;
                                }
                                else
                                {
                                    op.delete = dbValue;
                                }

                                if (primaryKeyValue != null)
                                {
                                    // Compound key that we use for lookup.
                                    // Consists of type of the table (for faster comparison that string names) + actual primary key of the row.
                                    var key = (table.ClientTableType, primaryKeyValue);

                                    if (primaryKeyChanges.TryGetValue(key, out var oldOp))
                                    {
                                        if ((op.insert is not null && oldOp.insert is not null) || (op.delete is not null && oldOp.delete is not null))
                                        {
                                            Logger.LogWarning($"Update with the same primary key was " +
                                                              $"applied multiple times! tableName={tableName}");
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

                        // Convert the generic event arguments in to a domain specific event object, this gets fed back into
                        // the message.TransactionUpdate.Event.FunctionCall.CallInfo field.
                        if (message.TypeCase == Message.TypeOneofCase.TransactionUpdate &&
                            deserializeEventCache.TryGetValue(message.TransactionUpdate.Event.FunctionCall.Reducer,
                                out var deserializer))
                        {
                            deserializer.Invoke(message.TransactionUpdate.Event);
                        }

                        break;
                    case ClientApi.Message.TypeOneofCase.IdentityToken:
                        break;
                    case ClientApi.Message.TypeOneofCase.Event:
                        break;
                    case ClientApi.Message.TypeOneofCase.OneOffQuery:
                        break;
                    case ClientApi.Message.TypeOneofCase.OneOffQueryResponse:
                        /// This case does NOT produce a list of DBOps, because it should not modify the client cache state!
                        var resp = message.OneOffQueryResponse;
                        var messageId = new Guid(resp.MessageId.Span);

                        if (!waitingOneOffQueries.Remove(messageId, out var resultSource))
                        {
                            Logger.LogError("Response to unknown one-off-query: " + messageId);
                            break;
                        }

                        resultSource.SetResult(resp);
                        break;
                }


                // Logger.LogWarning($"Total Updates preprocessed: {totalUpdateCount}");
                return new PreProcessedMessage { message = message, dbOps = dbOps, inserts = subscriptionInserts };
            }
        }

        struct ProcessedMessage
        {
            public Message message;
            public List<DbOp> dbOps;
        }

        // The message that has been preprocessed and has had its state diff calculated

        private BlockingCollection<ProcessedMessage> _stateDiffMessages = new BlockingCollection<ProcessedMessage>();
        private CancellationTokenSource _stateDiffCancellationTokenSource = new CancellationTokenSource();
        private CancellationToken _stateDiffCancellationToken;

        void ExecuteStateDiff()
        {
            while (!isClosing)
            {
                try
                {
                    var message = _preProcessedNetworkMessages.Take(_stateDiffCancellationToken);
                    var (m, events) = CalculateStateDiff(message);
                    _stateDiffMessages.Add(new ProcessedMessage { dbOps = events, message = m, });
                }
                catch (OperationCanceledException)
                {
                    // Normal shutdown
                    return;
                }
            }

            (Message, List<DbOp>) CalculateStateDiff(PreProcessedMessage preProcessedMessage)
            {
                var message = preProcessedMessage.message;
                var dbOps = preProcessedMessage.dbOps;
                // Perform the state diff, this has to be done on the main thread because we have to touch
                // the client cache.
                if (message.TypeCase == Message.TypeOneofCase.SubscriptionUpdate)
                {
                    foreach (var table in clientDB.GetTables())
                    {
                        if (!preProcessedMessage.inserts.TryGetValue(table.ClientTableType, out var hashSet))
                        {
                            continue;
                        }

                        foreach (var (rowBytes, oldValue) in table.entries.Where(kv => !hashSet.Contains(kv.Key)))
                        {
                            dbOps.Add(new DbOp
                            {
                                table = table,
                                // This is a row that we had before, but we do not have it now.
                                // This must have been a delete.
                                delete = new(oldValue, rowBytes),
                            });
                        }
                    }
                }

                return (message, dbOps);
            }
        }

        public void Close()
        {
            isClosing = true;
            connectionClosed = true;
            webSocket.Close();
            _preProcessCancellationTokenSource.Cancel();
            _stateDiffCancellationTokenSource.Cancel();

            webSocket = null;
        }

        /// <summary>
        /// Connect to a remote spacetime instance.
        /// </summary>
        /// <param name="uri"> URI of the SpacetimeDB server (ex: https://testnet.spacetimedb.com)
        /// <param name="addressOrName">The name or address of the database to connect to</param>
        public void Connect(string token, string uri, string addressOrName)
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

        private void OnMessageProcessCompleteUpdate(Message message, List<DbOp> dbOps)
        {
            var transactionEvent = message.TransactionUpdate?.Event!;

            // First trigger OnBeforeDelete
            foreach (var update in dbOps)
            {
                if (update.delete is { value: var oldValue })
                {
                    try
                    {
                        update.table.BeforeDeleteCallback?.Invoke(oldValue, transactionEvent);
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

                if (update.delete is {} delete)
                {
                    if (update.table.DeleteEntry(delete.bytes))
                    {
                        update.table.InternalValueDeletedCallback(delete.value);
                    }
                    else
                    {
                        update.delete = null;
                        dbOps[i] = update;
                    }
                }

                if (update.insert is {} insert)
                {
                    if (update.table.InsertEntry(insert.bytes, insert.value))
                    {
                        update.table.InternalValueInsertedCallback(insert.value);
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
                            dbOp.table.UpdateCallback?.Invoke(oldValue, newValue, transactionEvent);
                            break;

                        case { insert: { value: var newValue } }:
                            dbOp.table.InsertCallback?.Invoke(newValue, transactionEvent);
                            break;

                        case { delete: { value: var oldValue } }:
                            dbOp.table.DeleteCallback?.Invoke(oldValue, transactionEvent);
                            break;
                    }
                }
                catch (Exception e)
                {
                    Logger.LogException(e);
                }
            }
        }

        private void OnMessageProcessComplete(Message message, List<DbOp> dbOps)
        {
            switch (message.TypeCase)
            {
                case Message.TypeOneofCase.SubscriptionUpdate:
                    onBeforeSubscriptionApplied?.Invoke();
                    OnMessageProcessCompleteUpdate(message, dbOps);
                    try
                    {
                        onSubscriptionApplied?.Invoke();
                    }
                    catch (Exception e)
                    {
                        Logger.LogException(e);
                    }
                    break;
                case Message.TypeOneofCase.TransactionUpdate:
                    OnMessageProcessCompleteUpdate(message, dbOps);
                    try
                    {
                        onEvent?.Invoke(message.TransactionUpdate.Event);
                    }
                    catch (Exception e)
                    {
                        Logger.LogException(e);
                    }

                    bool reducerFound = false;
                    var functionName = message.TransactionUpdate.Event.FunctionCall.Reducer;
                    if (reducerEventCache.TryGetValue(functionName, out var value))
                    {
                        try
                        {
                            reducerFound = value.Invoke(message.TransactionUpdate.Event);
                        }
                        catch (Exception e)
                        {
                            Logger.LogException(e);
                        }
                    }

                    if (!reducerFound && message.TransactionUpdate.Event.Status ==
                        ClientApi.Event.Types.Status.Failed)
                    {
                        try
                        {
                            onUnhandledReducerError?.Invoke(message.TransactionUpdate.Event.FunctionCall
                                .CallInfo);
                        }
                        catch (Exception e)
                        {
                            Logger.LogException(e);
                        }
                    }

                    break;
                case Message.TypeOneofCase.IdentityToken:
                    try
                    {
                        onIdentityReceived?.Invoke(message.IdentityToken.Token,
                            Identity.From(message.IdentityToken.Identity.ToByteArray()),
                            (Address)Address.From(message.IdentityToken.Address.ToByteArray()));
                    }
                    catch (Exception e)
                    {
                        Logger.LogException(e);
                    }

                    break;
                case Message.TypeOneofCase.Event:
                    try
                    {
                        onEvent?.Invoke(message.Event);
                    }
                    catch (Exception e)
                    {
                        Logger.LogException(e);
                    }

                    break;
            }
        }

        private void OnMessageReceived(byte[] bytes) => _messageQueue.Add(bytes);

        public void InternalCallReducer(string json)
        {
            if (!webSocket.IsConnected)
            {
                Logger.LogError("Cannot call reducer, not connected to server!");
                return;
            }

            webSocket.Send(Encoding.ASCII.GetBytes("{ \"call\": " + json + " }"));
        }

        public void Subscribe(List<string> queries)
        {
            if (!webSocket.IsConnected)
            {
                Logger.LogError("Cannot subscribe, not connected to server!");
                return;
            }

            var json = JsonConvert.SerializeObject(queries);
            // should we use UTF8 here? ASCII is fragile.
            webSocket.Send(Encoding.ASCII.GetBytes("{ \"subscribe\": { \"query_strings\": " + json + " }}"));
        }

        /// Usage: SpacetimeDBClient.instance.OneOffQuery<Message>("WHERE sender = \"bob\"");
        public async Task<T[]> OneOffQuery<T>(string query) where T : IDatabaseTable
        {
            var messageId = Guid.NewGuid();
            var type = typeof(T);
            var resultSource = new TaskCompletionSource<OneOffQueryResponse>();
            waitingOneOffQueries[messageId] = resultSource;

            // unsanitized here, but writes will be prevented serverside.
            // the best they can do is send multiple selects, which will just result in them getting no data back.
            string queryString = "SELECT * FROM " + type.Name + " " + query;

            // see: SpacetimeDB\crates\core\src\client\message_handlers.rs, enum Message<'a>
            var serializedQuery = "{ \"one_off_query\": { \"message_id\": \"" +
                                  System.Convert.ToBase64String(messageId.ToByteArray()) +
                                  "\", \"query_string\": " + JsonConvert.SerializeObject(queryString) + " } }";
            webSocket.Send(Encoding.UTF8.GetBytes(serializedQuery));

            // Suspend for an arbitrary amount of time
            var result = await resultSource.Task;

            T[] LogAndThrow(string error)
            {
                error = "While processing one-off-query `" + queryString + "`, ID " + messageId + ": " + error;
                Logger.LogError(error);
                throw new Exception(error);
            }

            // The server got back to us
            if (result.Error != null && result.Error != "")
            {
                return LogAndThrow("Server error: " + result.Error);
            }

            if (result.Tables.Count != 1)
            {
                return LogAndThrow("Expected a single table, but got " + result.Tables.Count);
            }

            var resultTable = result.Tables[0];
            var cacheTable = clientDB.GetTable(resultTable.TableName);

            if (cacheTable.ClientTableType != type)
            {
                return LogAndThrow("Mismatched result type, expected " + type + " but got " + resultTable.TableName);
            }

            T[] results = (T[])Array.CreateInstance(type, resultTable.Row.Count);
            using var stream = new MemoryStream();
            using var reader = new BinaryReader(stream);
            for (int i = 0; i < results.Length; i++)
            {
                var rowValue = resultTable.Row[i].ToByteArray();
                stream.Position = 0;
                stream.Write(rowValue, 0, rowValue.Length);
                stream.Position = 0;
                stream.SetLength(rowValue.Length);

                var deserialized = AlgebraicValue.Deserialize(cacheTable.RowSchema, reader);
                cacheTable.SetAndForgetDecodedValue(deserialized, out var obj);
                results[i] = (T)obj;
            }

            return results;
        }

        public bool IsConnected() => webSocket != null && webSocket.IsConnected;

        public void Update()
        {
            webSocket.Update();
            while (_stateDiffMessages.TryTake(out var stateDiffMessage))
            {
                OnMessageProcessComplete(stateDiffMessage.message, stateDiffMessage.dbOps);
            }
        }
    }
}
