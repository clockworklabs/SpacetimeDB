using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Net.WebSockets;
using System.Reflection;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using ClientApi;
using Newtonsoft.Json;
using SpacetimeDB;
using SpacetimeDB.SATS;
using UnityEngine;
using Event = ClientApi.Event;

namespace SpacetimeDB
{
    public class NetworkManager : MonoBehaviour
    {
        public enum TableOp
        {
            Insert,
            Delete,
            Update
        }

        public class ReducerCallRequest
        {
            public string fn;
            public object[] args;
        }

        public class SubscriptionRequest
        {
            public string subscriptionQuery;
        }

        private struct DbEvent
        {
            public ClientCache.TableCache table;
            public byte[] rowPk;
            public TableOp op;
            public object oldValue;
            public object newValue;
        }

        public delegate void RowUpdate(string tableName, TableOp op, object oldValue, object newValue);

        /// <summary>
        /// Called when a connection is established to a spacetimedb instance.
        /// </summary>
        public event Action onConnect;

        /// <summary>
        /// Called when a connection attempt fails.
        /// </summary>
        public event Action<WebSocketError?> onConnectError;

        /// <summary>
        /// Called when an exception occurs when sending a message.
        /// </summary>
        public event Action<Exception> onSendError;

        /// <summary>
        /// Called when a connection that was established has disconnected.
        /// </summary>
        public event Action<WebSocketCloseStatus?, WebSocketError?> onDisconnect;

        /// <summary>
        /// Invoked on each row update to each table.
        /// </summary>
        public event RowUpdate onRowUpdate;

        /// <summary>
        /// Callback is invoked after a transaction or subscription update is received and all updates have been applied.
        /// </summary>
        public event Action onTransactionComplete;

        /// <summary>
        /// Called when we receive an identity from the server
        /// </summary>
        public event Action<Identity> onIdentityReceived;

        /// <summary>
        /// Invoked when an event message is received or at the end of a transaction update.
        /// </summary>
        public event Action<ClientApi.Event> onEvent;

        private SpacetimeDB.WebSocket webSocket;
        private bool connectionClosed;
        public static ClientCache clientDB;
        public static Dictionary<string, MethodInfo> reducerEventCache = new Dictionary<string, MethodInfo>();
        public static Dictionary<string, MethodInfo> deserializeEventCache = new Dictionary<string, MethodInfo>();

        private Thread messageProcessThread;

        public static NetworkManager instance;

        public string TokenKey
        {
            get { return GetTokenKey(); }
        }

        protected void Awake()
        {
            if (instance != null)
            {
                Debug.LogError($"There is more than one {GetType()}");
                return;
            }

            instance = this;

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
            webSocket.OnConnectError += a => onConnectError?.Invoke(a);
            webSocket.OnSendError += a => onSendError?.Invoke(a);

            clientDB = new ClientCache();

            var type = typeof(IDatabaseTable);
            var types = AppDomain.CurrentDomain.GetAssemblies()
                .SelectMany(s => s.GetTypes())
                .Where(p => type.IsAssignableFrom(p));
            foreach (var @class in types)
            {
                if (!@class.IsClass)
                {
                    continue;
                }

                var algebraicTypeFunc = @class.GetMethod("GetAlgebraicType", BindingFlags.Static | BindingFlags.Public);
                var algebraicValue = algebraicTypeFunc!.Invoke(null, null) as AlgebraicType;
                var conversionFunc = @class.GetMethods().FirstOrDefault(a =>
                    a.Name == "op_Explicit" && a.GetParameters().Length > 0 &&
                    a.GetParameters()[0].ParameterType == typeof(AlgebraicValue));
                clientDB.AddTable(@class, algebraicValue,
                    a => { return conversionFunc!.Invoke(null, new object[] { a }); });
            }

            // cache all our reducer events by their function name 
            foreach (var methodInfo in typeof(SpacetimeDB.Reducer).GetMethods())
            {
                if (methodInfo.GetCustomAttribute<ReducerEvent>() is { } reducerEvent)
                {
                    reducerEventCache.Add(reducerEvent.FunctionName, methodInfo);
                }

                if (methodInfo.GetCustomAttribute<DeserializeEvent>() is { } deserializeEvent)
                {
                    deserializeEventCache.Add(deserializeEvent.FunctionName, methodInfo);
                }
            }

            messageProcessThread = new Thread(ProcessMessages);
            messageProcessThread.Start();
        }

        struct ProcessedMessage
        {
            public Message message;
            public IList<DbEvent> events;
        }
        
        private readonly BlockingCollection<byte[]> _messageQueue = new BlockingCollection<byte[]>();
        private readonly ConcurrentQueue<ProcessedMessage> _completedMessages = new ConcurrentQueue<ProcessedMessage>();
        

        void ProcessMessages()
        {
            while (true)
            {
                var bytes = _messageQueue.Take();

                var message = ClientApi.Message.Parser.ParseFrom(bytes);
                SubscriptionUpdate subscriptionUpdate = null;
                switch (message.TypeCase)
                {
                    case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                        subscriptionUpdate = message.SubscriptionUpdate;
                        break;
                    case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                        subscriptionUpdate = message.TransactionUpdate.SubscriptionUpdate;
                        break;
                }

                switch (message.TypeCase)
                {
                    case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                    case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                    {
                        // First apply all of the state
                        System.Diagnostics.Debug.Assert(subscriptionUpdate != null,
                            nameof(subscriptionUpdate) + " != null");
                        using var stream = new MemoryStream();
                        using var reader = new BinaryReader(stream);
                        foreach (var update in subscriptionUpdate.TableUpdates)
                        {
                            foreach (var row in update.TableRowOperations)
                            {
                                stream.Position = 0;
                                stream.SetLength(row.Row.Length);
                                stream.Write(row.Row.ToByteArray(), 0, row.Row.Length);
                                stream.Position = 0;
                                var table = clientDB.GetTable(update.TableName);
                                var algebraicType = table.RowSchema;
                                var algebraicValue = AlgebraicValue.Deserialize(algebraicType, reader);
                                if (algebraicValue != null)
                                {
                                    // Here we are decoding on our message thread so that by the time we get to the
                                    // main thread the cache is already warm.
                                    table.Decode(row.RowPk.ToByteArray(), algebraicValue);
                                }
                            }
                        }
                    }
                        break;
                }

                var (m, events) = PreProcessMessage(bytes);
                _completedMessages.Enqueue(new ProcessedMessage
                {
                    message = m,
                    events = events,
                });
            }

            (Message, List<DbEvent>) PreProcessMessage(byte[] bytes)
            {
                var dbEvents = new List<DbEvent>();
                var message = ClientApi.Message.Parser.ParseFrom(bytes);
                using var stream = new MemoryStream();
                using var reader = new BinaryReader(stream);

                SubscriptionUpdate subscriptionUpdate = null;
                switch (message.TypeCase)
                {
                    case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                        subscriptionUpdate = message.SubscriptionUpdate;
                        break;
                    case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                        subscriptionUpdate = message.TransactionUpdate.SubscriptionUpdate;
                        break;
                }

                switch (message.TypeCase)
                {
                    case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                    case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                        // First apply all of the state
                        foreach (var update in subscriptionUpdate.TableUpdates)
                        {
                            var tableName = update.TableName;
                            var table = clientDB.GetTable(tableName);
                            if (table == null)
                            {
                                continue;
                            }

                            foreach (var row in update.TableRowOperations)
                            {
                                var rowPk = row.RowPk.ToByteArray();
                                var rowValue = row.Row.ToByteArray();
                                stream.Position = 0;
                                stream.Write(rowValue, 0, rowValue.Length);
                                stream.Position = 0;
                                stream.SetLength(rowValue.Length);

                                switch (row.Op)
                                {
                                    case TableRowOperation.Types.OperationType.Delete:
                                        if (table.entries.TryGetValue(rowPk, out var previousValue))
                                        {
                                            dbEvents.Add(new DbEvent
                                            {
                                                table = table,
                                                op = TableOp.Delete,
                                                newValue = null,
                                                oldValue = previousValue.Item2,
                                            });
                                        }

                                        break;
                                    case TableRowOperation.Types.OperationType.Insert:
                                        var algebraicValue = AlgebraicValue.Deserialize(table.RowSchema, reader);
                                        var decode = table.Decode(rowPk, algebraicValue);
                                        if (!table.entries.TryGetValue(rowPk, out _))
                                        {
                                            dbEvents.Add(new DbEvent
                                            {
                                                table = table,
                                                op = TableOp.Insert,
                                                newValue = decode.Item2,
                                                oldValue = null
                                            });
                                        }

                                        break;
                                }
                            }
                        }

                        break;
                    case ClientApi.Message.TypeOneofCase.IdentityToken:
                        break;
                    case ClientApi.Message.TypeOneofCase.Event:
                        break;
                }

                return (message, dbEvents);
            }
        }

        private void OnDestroy()
        {
            connectionClosed = true;
            webSocket.Close();
            webSocket = null;
        }

        /// <summary>
        /// Connect to a remote spacetime instance.
        /// </summary>
        /// <param name="host">The host or IP address and the port to connect to. Example: spacetime.spacetimedb.net:3000</param>
        /// <param name="addressOrName">The name or address of the database to connect to</param>
        /// <param name="sslEnabled">Should websocket use SSL</param>
        public void Connect(string host, string addressOrName, bool sslEnabled = true)
        {
            var token = PlayerPrefs.HasKey(GetTokenKey()) ? PlayerPrefs.GetString(GetTokenKey()) : null;

            Task.Run(async () =>
            {
                try
                {
                    await webSocket.Connect(token, host, addressOrName, sslEnabled);
                }
                catch (Exception e)
                {
                    if (connectionClosed)
                    {
                        Debug.Log("Connection closed gracefully.");
                        return;
                    }

                    Debug.LogException(e);
                }
            });
        }

        private void OnMessageProcessComplete(Message message, IList<DbEvent> events)
        {
            SubscriptionUpdate subscriptionUpdate = null;
            switch (message.TypeCase)
            {
                case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                    subscriptionUpdate = message.SubscriptionUpdate;
                    break;
                case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                    subscriptionUpdate = message.TransactionUpdate.SubscriptionUpdate;
                    break;
            }

            switch (message.TypeCase)
            {
                case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                    // First apply all of the state
                    foreach (var update in subscriptionUpdate.TableUpdates)
                    {
                        var tableName = update.TableName;
                        var table = clientDB.GetTable(tableName);
                        if (table == null)
                        {
                            continue;
                        }

                        foreach (var ev in events)
                        {
                            switch (ev.op)
                            {
                                case TableOp.Delete:
                                    table.Delete(ev.rowPk);
                                    break;
                                case TableOp.Insert:
                                    table.Insert(ev.rowPk);
                                    break;
                            }
                        }
                    }

                    // Send out events
                    var eventCount = events.Count;
                    for (var i = 0; i < eventCount; i++)
                    {
                        var tableName = events[i].table.ClientTableType.Name;
                        var tableOp = events[i].op;
                        object oldValue = events[i].oldValue, newValue = events[i].newValue;

                        switch (tableOp)
                        {
                            case TableOp.Insert:
                            {
                                if (events[i].table.InsertCallback != null)
                                {
                                    events[i].table.InsertCallback.Invoke(null, new [] { newValue });
                                }
                                break;
                            }
                            case TableOp.Delete:
                            {
                                if (events[i].table.DeleteCallback != null)
                                {
                                    events[i].table.DeleteCallback.Invoke(null, new [] { oldValue });
                                }
                                break;
                            }
                            case TableOp.Update:
                                throw new NotImplementedException();
                            default:
                                throw new ArgumentOutOfRangeException();
                        }

                        if (events[i].table.RowUpdatedCallback != null)
                        {
                            events[i].table.RowUpdatedCallback.Invoke(null, new[] { tableOp, oldValue, newValue });
                        }
                        onRowUpdate?.Invoke(tableName, tableOp, oldValue, newValue);
                    }

                    switch (message.TypeCase)
                    {
                        case ClientApi.Message.TypeOneofCase.SubscriptionUpdate:
                            onTransactionComplete?.Invoke();
                            break;
                        case ClientApi.Message.TypeOneofCase.TransactionUpdate:
                            onTransactionComplete?.Invoke();
                            onEvent?.Invoke(message.TransactionUpdate.Event);

                            var functionName = message.TransactionUpdate.Event.FunctionCall.Reducer;
                            if (reducerEventCache.ContainsKey(functionName))
                            {
                                reducerEventCache[functionName]
                                    .Invoke(null, new object[] { message.TransactionUpdate.Event });
                            }

                            break;
                    }

                    break;
                case ClientApi.Message.TypeOneofCase.IdentityToken:
                    onIdentityReceived?.Invoke(Identity.From(message.IdentityToken.Identity.ToByteArray()));
                    PlayerPrefs.SetString(GetTokenKey(), message.IdentityToken.Token);
                    break;
                case ClientApi.Message.TypeOneofCase.Event:
                    onEvent?.Invoke(message.Event);
                    break;
            }
        }


        private void OnMessageReceived(byte[] bytes) => _messageQueue.Add(bytes);

        private string GetTokenKey()
        {
            var key = "spacetimedb.identity_token";
#if UNITY_EDITOR
            // Different editors need different keys
            key += $" - {Application.dataPath}";
#endif
            return key;
        }

        internal void InternalCallReducer(string json)
        {
            webSocket.Send(Encoding.ASCII.GetBytes("{ \"call\": " + json + " }"));
        }

        public void Subscribe(List<string> queries)
        {
            var json = JsonConvert.SerializeObject(queries);
            webSocket.Send(Encoding.ASCII.GetBytes("{ \"subscribe\": { \"query_strings\": " + json + " }}"));
        }

        private void Update()
        {
            webSocket.Update();

            while (_completedMessages.TryDequeue(out var result))
            {
                OnMessageProcessComplete(result.message, result.events);
            }
        }
    }
}